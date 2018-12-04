use composed::{PrivateKey, PrivateSubKey, PublicKey, PublicSubKey};

/// This macro generates the parsers matching to the two different types of keys,
/// public and private.
macro_rules! key_parser {
    ( $key_type:ty, $key_type_parser: ident, $key_tag:expr, $inner_key_type:ty, $( ($subkey_tag:ident, $inner_subkey_type:ty, $subkey_type:ty, $subkey_container:ident) ),* ) => {
        /// Parse a transferable keys from the given packets.
        /// Ref: https://tools.ietf.org/html/rfc4880.html#section-11.1
        pub struct $key_type_parser<I: Sized + Iterator<Item = $crate::packet::Packet>> {
            inner: std::iter::Peekable<I>,
        }

        impl<I: Sized + Iterator<Item = $crate::packet::Packet>> Iterator for $key_type_parser<I> {
            type Item = $crate::errors::Result<$key_type>;

            fn next(&mut self) -> Option<Self::Item> {
                use try_from::TryInto;
                use $crate::packet::{self, Signature, SignatureType, UserAttribute, UserId};
                use $crate::types::{KeyVersion, SignedUser, SignedUserAttribute, Tag, KeyTrait};

                let packets = self.inner.by_ref();

                // -- One Public-Key packet

                // ignore random other packets until we find something useful
                while let Some(true) = packets.peek().map(|p| p.tag() != $key_tag) {
                    let p = packets.next().expect("peeked");
                    warn!("ignoring unexpected packet: expected {:?}, got {:?}", $key_tag, p.tag());
                }

                let next = match packets.next() {
                    Some(n) => n,
                    None => return None
                };
                info!("  primary key: {:#?}", next);
                let primary_key: $inner_key_type = err_opt!(next.try_into());
                info!("  {:?}", primary_key.key_id());

                // -- Zero or more revocation signatures
                // -- followed by zero or more direct signatures in V4 keys
                info!("  signatures");
                let mut revocation_signatures = Vec::new();
                let mut direct_signatures = Vec::new();

                while let Some(true) = packets.peek().map(|packet| packet.tag() == Tag::Signature) {
                    let packet = packets.next().expect("peeked");
                    info!("parsing signature {:?}", packet.tag());
                    let sig: Signature = err_opt!(packet.try_into());
                    let typ = sig.typ();

                    if typ == SignatureType::KeyRevocation {
                        revocation_signatures.push(sig);
                    } else {
                        if primary_key.version() != &KeyVersion::V4 {
                            // no direct signatures on V2|V3 keys
                            info!("WARNING: unexpected signature: {:?}", typ);
                        }
                        direct_signatures.push(sig);
                    }
                }

                // -- Zero or more User ID packets
                // -- Zero or more User Attribute packets
                info!("  user");
                let mut users = Vec::new();
                let mut user_attributes = Vec::new();

                while let Some(true) = packets
                    .peek()
                    .map(|packet| {
                        info!("peek {:?}", packet.tag());
                        packet.tag() == Tag::UserId || packet.tag() == Tag::UserAttribute
                    })
                {
                    let packet = packets.next().expect("peeked");
                    let tag = packet.tag();
                    info!("  user data: {:?}", tag);
                    match tag {
                        Tag::UserId => {
                            let id: UserId = err_opt!(packet.try_into());

                            // --- zero or more signature packets

                            let mut sigs = Vec::new();

                            while let Some(true) =
                                packets.peek().map(|packet| packet.tag() == Tag::Signature)
                            {
                                let packet = packets.next().expect("peeked");
                                let sig: Signature = err_opt!(packet.try_into());

                                sigs.push(sig);
                            }

                            users.push(SignedUser::new(id, sigs));
                        }
                        Tag::UserAttribute => {
                            let attr: UserAttribute = err_opt!(packet.try_into());

                            // --- zero or more signature packets

                            let mut sigs = Vec::new();
                            while let Some(true) =
                                packets.peek().map(|packet| packet.tag() == Tag::Signature)
                            {
                                let packet = packets.next().expect("peeked");
                                let sig: Signature = err_opt!(packet.try_into());

                                sigs.push(sig);
                            }

                            user_attributes.push(SignedUserAttribute::new(attr, sigs));
                        }
                        _ => break,
                    }
                }

                if users.is_empty() {
                    warn!("missing user ids");
                }

                // -- Zero or more Subkey packets
                $(
                    let mut $subkey_container = vec![];
                )*

                info!("  subkeys");

                while let Some(true) = packets.peek().map(|packet| {
                    $( packet.tag() == Tag::$subkey_tag || )* false
                })
                {
                    // -- Only V4 keys should have sub keys
                    if primary_key.version() != &KeyVersion::V4 {
                        return Some(Err(format_err!("only V4 keys can have subkeys")));
                    }

                    let packet = packets.next().expect("peeked");
                    match packet.tag() {
                        $(
                            Tag::$subkey_tag => {
                                let subkey: $inner_subkey_type = err_opt!(packet.try_into());
                                let mut sigs = Vec::new();
                                while let Some(true) =
                                    packets.peek().map(|packet| packet.tag() == Tag::Signature)
                                {
                                    let packet = packets.next().expect("peeked");
                                    let sig: Signature = err_opt!(packet.try_into());
                                    sigs.push(sig);
                                }

                                $subkey_container.push(<$subkey_type>::new(subkey, sigs));
                            }
                        )*
                            _ => unreachable!()
                    }
                }

                Some(Ok(<$key_type>::new(
                    primary_key,
                    revocation_signatures,
                    direct_signatures,
                    users,
                    user_attributes,
                    $( $subkey_container, )*
                )))
            }
        }

        impl $crate::composed::Deserializable for $key_type {
            /// Parse a transferable key from packets.
            /// Ref: https://tools.ietf.org/html/rfc4880.html#section-11.1
            fn from_packets<'a>(
                packets: impl Iterator<Item = $crate::packet::Packet> + 'a,
            ) -> Box<dyn Iterator<Item = $crate::errors::Result<Self>> + 'a> {
                Box::new($key_type_parser {
                    inner: packets.peekable(),
                })
            }
        }
    };
}

key_parser!(
    PrivateKey,
    PrivateKeyParser,
    Tag::SecretKey,
    packet::SecretKey,
    // secret keys, can contain both public and secret subkeys
    (
        PublicSubkey,
        packet::PublicSubkey,
        PublicSubKey,
        public_subkeys
    ),
    (
        SecretSubkey,
        packet::SecretSubkey,
        PrivateSubKey,
        private_subkeys
    )
);

key_parser!(
    PublicKey,
    PublicKeyParser,
    Tag::PublicKey,
    packet::PublicKey,
    (
        PublicSubkey,
        packet::PublicSubkey,
        PublicSubKey,
        public_subkeys
    )
);
