use std::fmt;

use byteorder::{BigEndian, ByteOrder};
use chrono::{DateTime, Utc};
use num_traits::FromPrimitive;

use crypto::aead::AeadAlgorithm;
use crypto::hash::{HashAlgorithm, Hasher};
use crypto::public_key::PublicKeyAlgorithm;
use crypto::sym::SymmetricKeyAlgorithm;
use errors::Result;
use packet::PacketTrait;
use ser::Serialize;
use types::{self, CompressionAlgorithm, KeyId, PublicKeyTrait, Tag, Version};

/// Signature Packet
/// https://tools.ietf.org/html/rfc4880.html#section-5.2
#[derive(Clone, PartialEq, Eq)]
pub struct Signature {
    packet_version: Version,
    pub version: SignatureVersion,
    pub typ: SignatureType,
    pub pub_alg: PublicKeyAlgorithm,
    pub hash_alg: HashAlgorithm,
    pub signed_hash_value: [u8; 2],
    pub signature: Vec<Vec<u8>>,

    // only set on V2 and V3 keys
    pub created: Option<DateTime<Utc>>,
    pub issuer: Option<KeyId>,

    pub unhashed_subpackets: Vec<Subpacket>,
    pub hashed_subpackets: Vec<Subpacket>,
}

impl Signature {
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::complexity))]
    pub fn new(
        packet_version: Version,
        version: SignatureVersion,
        typ: SignatureType,
        pub_alg: PublicKeyAlgorithm,
        hash_alg: HashAlgorithm,
        signed_hash_value: [u8; 2],
        signature: Vec<Vec<u8>>,
        hashed_subpackets: Vec<Subpacket>,
        unhashed_subpackets: Vec<Subpacket>,
    ) -> Self {
        Signature {
            packet_version,
            version,
            typ,
            pub_alg,
            hash_alg,
            signed_hash_value,
            signature,
            hashed_subpackets,
            unhashed_subpackets,
            issuer: None,
            created: None,
        }
    }

    /// Returns what kind of signature this is.
    pub fn typ(&self) -> SignatureType {
        self.typ
    }

    /// Verify this signature.
    pub fn verify(&self, key: &impl PublicKeyTrait, data: &[u8]) -> Result<()> {
        if let Some(key_id) = key.key_id() {
            if let Some(issuer) = self.issuer() {
                if &key_id != issuer {
                    // TODO: should this be an actual error?
                    warn!(
                        "validating signature with a non matching Key ID {:?} != {:?}",
                        &key_id, issuer
                    );
                }
            }
        }

        let mut hasher = self.hash_alg.new_hasher()?;

        self.hash_data_to_sign(&mut *hasher, data)?;
        let len = self.hash_signature_data(&mut *hasher)?;
        hasher.update(&self.trailer(len));

        let hash = &hasher.finish()[..];
        ensure_eq!(&self.signed_hash_value, &hash[0..2]);

        key.verify_signature(self.hash_alg, hash, &self.signature)
    }

    /// Verifies a certificate siganture type.
    pub fn verify_certificate(
        &self,
        key: &impl PublicKeyTrait,
        tag: Tag,
        id: &impl Serialize,
    ) -> Result<()> {
        info!("verifying certificate");

        if let Some(key_id) = key.key_id() {
            if let Some(issuer) = self.issuer() {
                if &key_id != issuer {
                    // TODO: should this be an actual error?
                    warn!(
                        "validating certificate with a non matching Key ID {:?} != {:?}",
                        &key_id, issuer
                    );
                }
            }
        }

        let mut hasher = self.hash_alg.new_hasher()?;
        let mut key_buf = Vec::new();
        key.to_writer(&mut key_buf)?;

        let mut packet_buf = Vec::new();
        id.to_writer(&mut packet_buf)?;

        info!(
            "key:    ({:?}), {}",
            key.key_id().expect("key_id should be there"),
            hex::encode(&key_buf)
        );
        info!("packet: {}", hex::encode(&packet_buf));

        // old style packet header for the key
        hasher.update(&[0x99]);
        hasher.update(&[(key_buf.len() >> 8) as u8, key_buf.len() as u8]);
        // the actual key
        hasher.update(&key_buf);

        match self.version {
            SignatureVersion::V2 | SignatureVersion::V3 => {
                // Nothing to do
            }
            SignatureVersion::V4 | SignatureVersion::V5 => {
                let prefix = match tag {
                    Tag::UserId => 0xB4,
                    Tag::UserAttribute => 0xD1,
                    _ => bail!("invalid tag for certificate validation: {:?}", tag),
                };

                let mut prefix_buf = [prefix, 0u8, 0u8, 0u8, 0u8];
                BigEndian::write_u32(&mut prefix_buf[1..], packet_buf.len() as u32);
                info!("prefix: {}", hex::encode(&prefix_buf));

                // prefixes
                hasher.update(&prefix_buf);
            }
        }

        // the packet content
        hasher.update(&packet_buf);

        let len = self.hash_signature_data(&mut *hasher)?;
        hasher.update(&self.trailer(len));

        let hash = &hasher.finish()[..];
        ensure_eq!(&self.signed_hash_value, &hash[0..2]);

        key.verify_signature(self.hash_alg, hash, &self.signature)
    }

    /// Verifies a key binding.
    pub fn verify_key_binding(
        &self,
        signing_key: &impl PublicKeyTrait,
        key: &impl PublicKeyTrait,
    ) -> Result<()> {
        info!(
            "verifying key binding: {:#?} - {:#?} - {:#?}",
            self, signing_key, key
        );

        if let Some(key_id) = signing_key.key_id() {
            if let Some(issuer) = self.issuer() {
                if &key_id != issuer {
                    // TODO: should this be an actual error?
                    warn!(
                        "validating key binding with a non matching Key ID {:?} != {:?}",
                        &key_id, issuer
                    );
                }
            }
        }

        let mut hasher = self.hash_alg.new_hasher()?;

        // Signing Key
        {
            let mut key_buf = Vec::new();
            signing_key.to_writer(&mut key_buf)?;

            // old style packet header for the key
            hasher.update(&[0x99]);
            hasher.update(&[(key_buf.len() >> 8) as u8, key_buf.len() as u8]);
            // the actual key
            hasher.update(&key_buf);
        }
        // Key being bound
        {
            let mut key_buf = Vec::new();
            key.to_writer(&mut key_buf)?;

            // old style packet header for the key
            hasher.update(&[0x99]);
            hasher.update(&[(key_buf.len() >> 8) as u8, key_buf.len() as u8]);
            // the actual key
            hasher.update(&key_buf);
        }

        let len = self.hash_signature_data(&mut *hasher)?;
        hasher.update(&self.trailer(len));

        let hash = &hasher.finish()[..];
        ensure_eq!(&self.signed_hash_value, &hash[0..2]);

        signing_key.verify_signature(self.hash_alg, hash, &self.signature)
    }

    /// Verifies a direct key signature or a revocatio.
    pub fn verify_key(&self, key: &impl PublicKeyTrait) -> Result<()> {
        info!("verifying key (revocation): {:#?} - {:#?}", self, key);

        if let Some(key_id) = key.key_id() {
            if let Some(issuer) = self.issuer() {
                if &key_id != issuer {
                    // TODO: should this be an actual error?
                    warn!(
                        "validating key (revocation) with a non matching Key ID {:?} != {:?}",
                        &key_id, issuer
                    );
                }
            }
        }

        let mut hasher = self.hash_alg.new_hasher()?;

        {
            let mut key_buf = Vec::new();
            key.to_writer(&mut key_buf)?;

            // old style packet header for the key
            hasher.update(&[0x99]);
            hasher.update(&[(key_buf.len() >> 8) as u8, key_buf.len() as u8]);
            // the actual key
            hasher.update(&key_buf);
        }

        let len = self.hash_signature_data(&mut *hasher)?;
        hasher.update(&self.trailer(len));

        let hash = &hasher.finish()[..];
        ensure_eq!(&self.signed_hash_value, &hash[0..2]);

        key.verify_signature(self.hash_alg, hash, &self.signature)
    }

    /// Calcluate the serialized version of this packet, but only the part relevant for hashing.
    fn hash_signature_data(&self, hasher: &mut dyn Hasher) -> Result<usize> {
        match self.version {
            SignatureVersion::V2 | SignatureVersion::V3 => {
                let mut buf = [0u8; 5];
                buf[0] = self.typ as u8;
                BigEndian::write_u32(
                    &mut buf[1..],
                    self.created
                        .expect("must exist for a v3 signature")
                        .timestamp() as u32,
                );

                hasher.update(&buf);

                // no trailer
                Ok(0)
            }
            SignatureVersion::V4 | SignatureVersion::V5 => {
                // TODO: validate this is the right thing to do for v5
                // TODO: reduce duplication with serialization code

                let mut res = vec![
                    // version
                    self.version as u8,
                    // type
                    self.typ as u8,
                    // public algorithm
                    self.pub_alg as u8,
                    // hash algorithm
                    self.hash_alg as u8,
                    // will be filled with the length
                    0u8,
                    0u8,
                ];

                // hashed subpackets
                let mut hashed_subpackets = Vec::new();
                for packet in &self.hashed_subpackets {
                    packet.to_writer(&mut hashed_subpackets)?;
                }

                BigEndian::write_u16(&mut res[4..6], hashed_subpackets.len() as u16);
                res.extend(hashed_subpackets);

                hasher.update(&res);

                Ok(res.len())
            }
        }
    }

    fn hash_data_to_sign(&self, hasher: &mut dyn Hasher, data: &[u8]) -> Result<usize> {
        match self.typ {
            SignatureType::Binary => {
                hasher.update(data);
                Ok(data.len())
            }
            SignatureType::Text => unimplemented_err!("Text"),
            SignatureType::Standalone => unimplemented_err!("Standalone"),
            SignatureType::CertGeneric => unimplemented_err!("CertGeneric"),
            SignatureType::CertPersona => unimplemented_err!("CertPersona"),
            SignatureType::CertCasual => unimplemented_err!("CertCasual"),
            SignatureType::CertPositive => unimplemented_err!("CertPositive"),
            SignatureType::SubkeyBinding => unimplemented_err!("SubkeyBinding"),
            SignatureType::KeyBinding => unimplemented_err!("KeyBinding"),
            SignatureType::Key => unimplemented_err!("Key"),
            SignatureType::KeyRevocation => unimplemented_err!("KeyRevocation"),
            SignatureType::CertRevocation => unimplemented_err!("CertRevocation"),
            SignatureType::Timestamp => unimplemented_err!("Timestamp"),
            SignatureType::ThirdParty => unimplemented_err!("ThirdParty"),
            SignatureType::SubkeyRevocation => unimplemented_err!("SubkeyRevocation"),
        }
    }

    fn trailer(&self, len: usize) -> Vec<u8> {
        match self.version {
            SignatureVersion::V2 | SignatureVersion::V3 => {
                // Nothing to do
                Vec::new()
            }
            SignatureVersion::V4 | SignatureVersion::V5 => {
                let mut trailer = vec![0x04, 0xFF, 0, 0, 0, 0];
                BigEndian::write_u32(&mut trailer[2..], len as u32);
                trailer
            }
        }
    }

    /// Returns if the signature is a certificate or not.
    pub fn is_certificate(&self) -> bool {
        match self.typ {
            SignatureType::CertGeneric
            | SignatureType::CertPersona
            | SignatureType::CertCasual
            | SignatureType::CertPositive
            | SignatureType::CertRevocation => true,
            _ => false,
        }
    }

    /// Returns an iterator over all subpackets of this signature.
    fn subpackets(&self) -> impl Iterator<Item = &Subpacket> {
        self.hashed_subpackets
            .iter()
            .chain(self.unhashed_subpackets.iter())
    }

    pub fn key_expiration_time(&self) -> Option<&DateTime<Utc>> {
        self.subpackets().find_map(|p| match p {
            Subpacket::KeyExpirationTime(d) => Some(d),
            _ => None,
        })
    }

    pub fn signature_expiration_time(&self) -> Option<&DateTime<Utc>> {
        self.subpackets().find_map(|p| match p {
            Subpacket::SignatureExpirationTime(d) => Some(d),
            _ => None,
        })
    }

    pub fn created(&self) -> Option<&DateTime<Utc>> {
        if self.created.is_some() {
            return self.created.as_ref();
        }

        self.subpackets().find_map(|p| match p {
            Subpacket::SignatureCreationTime(d) => Some(d),
            _ => None,
        })
    }

    pub fn issuer(&self) -> Option<&KeyId> {
        if self.issuer.is_some() {
            return self.issuer.as_ref();
        }

        self.subpackets().find_map(|p| match p {
            Subpacket::Issuer(id) => Some(id),
            _ => None,
        })
    }

    pub fn preferred_symmetric_algs(&self) -> &[SymmetricKeyAlgorithm] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::PreferredSymmetricAlgorithms(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn preferred_hash_algs(&self) -> &[HashAlgorithm] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::PreferredHashAlgorithms(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn preferred_compression_algs(&self) -> &[CompressionAlgorithm] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::PreferredCompressionAlgorithms(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn key_server_prefs(&self) -> &[u8] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::KeyServerPreferences(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn key_flags(&self) -> &[u8] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::KeyFlags(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn features(&self) -> &[u8] {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::Features(d) => Some(&d[..]),
                _ => None,
            })
            .unwrap_or_else(|| &[][..])
    }

    pub fn revocation_reason_code(&self) -> Option<&RevocationCode> {
        self.subpackets().find_map(|p| match p {
            Subpacket::RevocationReason(code, _) => Some(code),
            _ => None,
        })
    }

    pub fn revocation_reason_string(&self) -> Option<&str> {
        self.subpackets().find_map(|p| match p {
            Subpacket::RevocationReason(_, reason) => Some(reason.as_str()),
            _ => None,
        })
    }

    pub fn is_primary(&self) -> bool {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::IsPrimary(d) => Some(*d),
                _ => None,
            })
            .unwrap_or_else(|| false)
    }

    pub fn is_revocable(&self) -> bool {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::Revocable(d) => Some(*d),
                _ => None,
            })
            .unwrap_or_else(|| true)
    }

    pub fn embedded_signature(&self) -> Option<&Signature> {
        self.subpackets().find_map(|p| match p {
            Subpacket::EmbeddedSignature(d) => Some(&**d),
            _ => None,
        })
    }

    pub fn preferred_key_server(&self) -> Option<&str> {
        self.subpackets().find_map(|p| match p {
            Subpacket::PreferredKeyServer(d) => Some(d.as_str()),
            _ => None,
        })
    }

    pub fn notations(&self) -> Vec<&Notation> {
        self.subpackets()
            .filter_map(|p| match p {
                Subpacket::Notation(d) => Some(d),
                _ => None,
            })
            .collect()
    }

    pub fn revocation_key(&self) -> Option<&types::RevocationKey> {
        self.subpackets().find_map(|p| match p {
            Subpacket::RevocationKey(d) => Some(d),
            _ => None,
        })
    }

    pub fn signers_userid(&self) -> Option<&str> {
        self.subpackets().find_map(|p| match p {
            Subpacket::SignersUserID(d) => Some(d.as_str()),
            _ => None,
        })
    }

    pub fn policy_uri(&self) -> Option<&str> {
        self.subpackets().find_map(|p| match p {
            Subpacket::PolicyURI(d) => Some(d.as_str()),
            _ => None,
        })
    }

    pub fn trust_signature(&self) -> Option<(u8, u8)> {
        self.subpackets().find_map(|p| match p {
            Subpacket::TrustSignature(depth, value) => Some((*depth, *value)),
            _ => None,
        })
    }

    pub fn regular_expression(&self) -> Option<&str> {
        self.subpackets().find_map(|p| match p {
            Subpacket::RegularExpression(d) => Some(d.as_str()),
            _ => None,
        })
    }

    pub fn exportable_certification(&self) -> bool {
        self.subpackets()
            .find_map(|p| match p {
                Subpacket::ExportableCertification(d) => Some(*d),
                _ => None,
            })
            .unwrap_or_else(|| true)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum SignatureVersion {
    /// Deprecated
    V2 = 2,
    V3 = 3,
    V4 = 4,
    V5 = 5,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, FromPrimitive)]
#[repr(u8)]
pub enum SignatureType {
    /// Signature of a binary document.
    /// This means the signer owns it, created it, or certifies that ithas not been modified.
    Binary = 0x00,
    /// Signature of a canonical text document.
    /// This means the signer owns it, created it, or certifies that it
    /// has not been modified.  The signature is calculated over the text
    /// data with its line endings converted to <CR><LF>.
    Text = 0x01,
    /// Standalone signature.
    /// This signature is a signature of only its own subpacket contents.
    /// It is calculated identically to a signature over a zero-length
    /// binary document.  Note that it doesn't make sense to have a V3 standalone signature.
    Standalone = 0x02,
    /// Generic certification of a User ID and Public-Key packet.
    /// The issuer of this certification does not make any particular
    /// assertion as to how well the certifier has checked that the owner
    /// of the key is in fact the person described by the User ID.
    CertGeneric = 0x10,
    /// Persona certification of a User ID and Public-Key packet.
    /// The issuer of this certification has not done any verification of
    /// the claim that the owner of this key is the User ID specified.
    CertPersona = 0x11,
    /// Casual certification of a User ID and Public-Key packet.
    /// The issuer of this certification has done some casual
    /// verification of the claim of identity.
    CertCasual = 0x12,
    /// Positive certification of a User ID and Public-Key packet.
    /// The issuer of this certification has done substantial
    /// verification of the claim of identity.
    ///
    /// Most OpenPGP implementations make their "key signatures" as 0x10
    /// certifications.  Some implementations can issue 0x11-0x13
    /// certifications, but few differentiate between the types.
    CertPositive = 0x13,
    /// Subkey Binding Signature
    /// This signature is a statement by the top-level signing key that
    /// indicates that it owns the subkey.  This signature is calculated
    /// directly on the primary key and subkey, and not on any User ID or
    /// other packets.  A signature that binds a signing subkey MUST have
    /// an Embedded Signature subpacket in this binding signature that
    /// contains a 0x19 signature made by the signing subkey on the
    /// primary key and subkey.
    SubkeyBinding = 0x18,
    /// Primary Key Binding Signature
    /// This signature is a statement by a signing subkey, indicating
    /// that it is owned by the primary key and subkey.  This signature
    /// is calculated the same way as a 0x18 signature: directly on the
    /// primary key and subkey, and not on any User ID or other packets.
    KeyBinding = 0x19,
    /// Signature directly on a key
    /// This signature is calculated directly on a key.  It binds the
    /// information in the Signature subpackets to the key, and is
    /// appropriate to be used for subpackets that provide information
    /// about the key, such as the Revocation Key subpacket.  It is also
    /// appropriate for statements that non-self certifiers want to make
    /// about the key itself, rather than the binding between a key and a name.
    Key = 0x1F,
    /// Key revocation signature
    /// The signature is calculated directly on the key being revoked.  A
    /// revoked key is not to be used.  Only revocation signatures by the
    /// key being revoked, or by an authorized revocation key, should be
    /// considered valid revocation signatures.
    KeyRevocation = 0x20,
    /// Subkey revocation signature
    /// The signature is calculated directly on the subkey being revoked.
    /// A revoked subkey is not to be used.  Only revocation signatures
    /// by the top-level signature key that is bound to this subkey, or
    /// by an authorized revocation key, should be considered valid
    /// revocation signatures.
    SubkeyRevocation = 0x28,
    /// Certification revocation signature
    /// This signature revokes an earlier User ID certification signature
    /// (signature class 0x10 through 0x13) or direct-key signature
    /// (0x1F).  It should be issued by the same key that issued the
    /// revoked signature or an authorized revocation key.  The signature
    /// is computed over the same data as the certificate that it
    /// revokes, and should have a later creation date than that
    /// certificate.
    CertRevocation = 0x30,
    /// Timestamp signature.
    /// This signature is only meaningful for the timestamp contained in
    /// it.
    Timestamp = 0x40,
    /// Third-Party Confirmation signature.
    /// This signature is a signature over some other OpenPGP Signature
    /// packet(s).  It is analogous to a notary seal on the signed data.
    /// A third-party signature SHOULD include Signature Target
    /// subpacket(s) to give easy identification.  Note that we really do
    /// mean SHOULD.  There are plausible uses for this (such as a blind
    /// party that only sees the signature, not the key or source
    /// document) that cannot include a target subpacket.
    ThirdParty = 0x50,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
/// Available signature subpacket types
pub enum SubpacketType {
    SignatureCreationTime,
    SignatureExpirationTime,
    ExportableCertification,
    TrustSignature,
    RegularExpression,
    Revocable,
    KeyExpirationTime,
    PreferredSymmetricAlgorithms,
    RevocationKey,
    Issuer,
    Notation,
    PreferredHashAlgorithms,
    PreferredCompressionAlgorithms,
    KeyServerPreferences,
    PreferredKeyServer,
    PrimaryUserId,
    PolicyURI,
    KeyFlags,
    SignersUserID,
    RevocationReason,
    Features,
    SignatureTarget,
    EmbeddedSignature,
    IssuerFingerprint,
    PreferredAead,
    Experimental(u8),
    Other(u8),
}

impl Into<u8> for SubpacketType {
    #[inline]
    fn into(self) -> u8 {
        match self {
            SubpacketType::SignatureCreationTime => 2,
            SubpacketType::SignatureExpirationTime => 3,
            SubpacketType::ExportableCertification => 4,
            SubpacketType::TrustSignature => 5,
            SubpacketType::RegularExpression => 6,
            SubpacketType::Revocable => 7,
            SubpacketType::KeyExpirationTime => 9,
            SubpacketType::PreferredSymmetricAlgorithms => 11,
            SubpacketType::RevocationKey => 12,
            SubpacketType::Issuer => 16,
            SubpacketType::Notation => 20,
            SubpacketType::PreferredHashAlgorithms => 21,
            SubpacketType::PreferredCompressionAlgorithms => 22,
            SubpacketType::KeyServerPreferences => 23,
            SubpacketType::PreferredKeyServer => 24,
            SubpacketType::PrimaryUserId => 25,
            SubpacketType::PolicyURI => 26,
            SubpacketType::KeyFlags => 27,
            SubpacketType::SignersUserID => 28,
            SubpacketType::RevocationReason => 29,
            SubpacketType::Features => 30,
            SubpacketType::SignatureTarget => 31,
            SubpacketType::EmbeddedSignature => 32,
            SubpacketType::IssuerFingerprint => 33,
            SubpacketType::PreferredAead => 34,
            SubpacketType::Experimental(n) => n,
            SubpacketType::Other(n) => n,
        }
    }
}

impl FromPrimitive for SubpacketType {
    #[inline]
    fn from_i64(n: i64) -> Option<Self> {
        if n > 0 && n < 256 {
            Self::from_u64(n as u64)
        } else {
            None
        }
    }

    #[inline]
    fn from_u64(n: u64) -> Option<Self> {
        if n > 255 {
            None
        } else {
            let m = match n {
                2 => SubpacketType::SignatureCreationTime,
                3 => SubpacketType::SignatureExpirationTime,
                4 => SubpacketType::ExportableCertification,
                5 => SubpacketType::TrustSignature,
                6 => SubpacketType::RegularExpression,
                7 => SubpacketType::Revocable,
                9 => SubpacketType::KeyExpirationTime,
                11 => SubpacketType::PreferredSymmetricAlgorithms,
                12 => SubpacketType::RevocationKey,
                16 => SubpacketType::Issuer,
                20 => SubpacketType::Notation,
                21 => SubpacketType::PreferredHashAlgorithms,
                22 => SubpacketType::PreferredCompressionAlgorithms,
                23 => SubpacketType::KeyServerPreferences,
                24 => SubpacketType::PreferredKeyServer,
                25 => SubpacketType::PrimaryUserId,
                26 => SubpacketType::PolicyURI,
                27 => SubpacketType::KeyFlags,
                28 => SubpacketType::SignersUserID,
                29 => SubpacketType::RevocationReason,
                30 => SubpacketType::Features,
                31 => SubpacketType::SignatureTarget,
                32 => SubpacketType::EmbeddedSignature,
                33 => SubpacketType::IssuerFingerprint,
                34 => SubpacketType::PreferredAead,
                100...110 => SubpacketType::Experimental(n as u8),
                _ => SubpacketType::Other(n as u8),
            };

            Some(m)
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Subpacket {
    /// The time the signature was made.
    SignatureCreationTime(DateTime<Utc>),
    /// The time the signature will expire.
    SignatureExpirationTime(DateTime<Utc>),
    /// When the key is going to expire
    KeyExpirationTime(DateTime<Utc>),
    Issuer(KeyId),
    /// List of symmetric algorithms that indicate which algorithms the key holder prefers to use.
    PreferredSymmetricAlgorithms(Vec<SymmetricKeyAlgorithm>),
    /// List of hash algorithms that indicate which algorithms the key holder prefers to use.
    PreferredHashAlgorithms(Vec<HashAlgorithm>),
    /// List of compression algorithms that indicate which algorithms the key holder prefers to use.
    PreferredCompressionAlgorithms(Vec<CompressionAlgorithm>),
    KeyServerPreferences(Vec<u8>),
    KeyFlags(Vec<u8>),
    Features(Vec<u8>),
    RevocationReason(RevocationCode, String),
    IsPrimary(bool),
    Revocable(bool),
    EmbeddedSignature(Box<Signature>),
    PreferredKeyServer(String),
    Notation(Notation),
    RevocationKey(types::RevocationKey),
    SignersUserID(String),
    PolicyURI(String),
    TrustSignature(u8, u8),
    RegularExpression(String),
    ExportableCertification(bool),
    IssuerFingerprint(Vec<u8>),
    PreferredAeadAlgorithms(Vec<AeadAlgorithm>),
    Experimental(u8, Vec<u8>),
    Other(u8, Vec<u8>),
    SignatureTarget(PublicKeyAlgorithm, HashAlgorithm, Vec<u8>),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Notation {
    pub readable: bool,
    pub name: String,
    pub value: String,
}

/// Codes for revocation reasons
#[derive(Debug, PartialEq, Eq, Copy, Clone, FromPrimitive)]
#[repr(u8)]
pub enum RevocationCode {
    /// No reason specified (key revocations or cert revocations)
    NoReason = 0,
    /// Key is superseded (key revocations)
    KeySuperseded = 1,
    /// Key material has been compromised (key revocations)
    KeyCompromised = 2,
    /// Key is retired and no longer used (key revocations)
    KeyRetired = 3,
    /// User ID information is no longer valid (cert revocations)
    CertUserIdInvalid = 32,
}

#[derive(FromPrimitive)]
/// Available key flags
pub enum KeyFlag {
    /// This key may be used to certify other keys.
    CertifyKeys = 0x01,
    /// This key may be used to sign data.
    SignData = 0x02,
    /// This key may be used to encrypt communications.
    EncryptCommunication = 0x04,
    /// This key may be used to encrypt storage.
    EncryptStorage = 0x08,
    /// The private component of this key may have been split by a secret-sharing mechanism.
    SplitPrivateKey = 0x10,
    /// This key may be used for authentication.
    Authentication = 0x20,
    /// The private component of this key may be in the possession of more than one person.
    SharedPrivateKey = 0x80,
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Signature")
            .field("packet_version", &self.packet_version)
            .field("version", &self.version)
            .field("typ", &self.typ)
            .field("pub_alg", &self.pub_alg)
            .field("hash_alg", &self.hash_alg)
            .field("signed_hash_value", &hex::encode(&self.signed_hash_value))
            .field("signature", &hex::encode(&self.signature.concat()))
            .field("created", &self.created)
            .field("issuer", &self.issuer)
            .field("unhashed_subpackets", &self.unhashed_subpackets)
            .field("hashed_subpackets", &self.hashed_subpackets)
            .finish()
    }
}

impl PacketTrait for Signature {
    fn packet_version(&self) -> Version {
        self.packet_version
    }

    fn tag(&self) -> Tag {
        Tag::Signature
    }
}
