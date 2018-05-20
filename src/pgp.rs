// use nom::IResult;
// use std::str;

// use packet::{Packet, packet_parser};
// use key::Key;
// use armor;


// fn parse_key_raw<'a>(input: &'a [u8]) -> IResult<&'a [u8], armor::Block<'a>> {
//     armor::parse(input).map(|(typ, headers, body)| {
//         // TODO: Proper error handling
//         // println!("start: {:#08b} {:#08b}", body[0], body[1]);

//         let res = packets_parser(body.as_slice());
//         if res.is_err() {
//             println!("failed to parse packets: {:?}", res);
//         }

//         let (_, packets) = res.unwrap();
//         armor::Block {
//             typ: typ,
//             headers: headers,
//             packets: packets,
//         }
//     })
// }

// /// parse a key dump in the format retunred from `gpg --export`.
// pub fn parse_keys(input: &[u8]) -> IResult<&[u8], Vec<Key>> {
//     let (_, packets) = packets_parser(input).unwrap();
//     Key::from_packets(packets)
// }


// // TODO: change to regular result
// pub fn parse_key(input: &[u8]) -> Result<Key> {
//     let (_, block) = parse_key_raw(input).unwrap();

//     Key::from_block(block).map(|mut keys| keys.remove(0))
// }


// #[cfg(test)]
// mod tests {
//     use super::*;
//     use packet::types::{Signature, SignatureVersion, SignatureType, User, PublicKey, PrimaryKey,
//                         KeyVersion, PublicKeyAlgorithm, HashAlgorithm, Subpacket,
//                         SymmetricKeyAlgorithm, CompressionAlgorithm, UserAttributeType};
//     use chrono::{DateTime, Utc};
//     use std::fs::File;
//     use std::io::Read;

//     fn get_test_key(name: &str) -> Vec<u8> {
//         let dir = format!("./tests/opengpg-interop/testcases/key/{}", name);

//         let mut f = File::open(name).expect("unable to open file");
//         let mut buf = Vec::new();
//         f.read_to_end(&mut buf).unwrap();

//         buf
//     }
//     // #[test]
//     // fn test_parse_dump() {
//     //     for i in 0..10 {
//     //         let name = format!("./tests/sks-dump/000{}.pgp", i);
//     //         println!("reading: {:?}", name);
//     //         let mut f = File::open(name).expect("file not found");
//     //         let mut buf = Vec::new();
//     //         f.read_to_end(&mut buf).unwrap();

//     //         parse_keys(buf.as_slice()).unwrap();
//     //     }
//     // }

//     #[test]
//     fn test_parse_gnupg_v1() {
//         for i in 0..4 {
//             let name = format!("gnupg-v1-00{}.asc", i);
//             let key = Key::from_armor_bytes();

//             key.expect("failed to parse key");
//         }
//     }

//     #[test]
//     fn test_parse_e2e() {
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/e2e-001.asc"),
//         ).unwrap();
//     }

//     #[test]
//     fn test_parse_openkeychain() {
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/openkeychain-001.asc"),
//         ).unwrap();
//     }

//     #[test]
//     fn test_parse_pgp() {
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/pgp-6-5-001.asc"),
//         ).unwrap();
//     }

//     #[test]
//     fn test_parse_subkey() {
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/subkey-001.asc"),
//         ).unwrap();
//     }

//     #[test]
//     fn test_parse_uid() {
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/uid-001.asc"),
//         ).unwrap();
//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/uid-002.asc"),
//         ).unwrap();

//         parse_key(
//             include_bytes!("../tests/opengpg-interop/testcases/keys/uid-003.asc"),
//         ).unwrap();
//     }

//     #[test]
//     fn test_parse() {
//         let raw = include_bytes!("../tests/opengpg-interop/testcases/keys/gnupg-v1-003.asc");
//         let (_, key) = parse_key(raw).unwrap();

//         // assert_eq!(key.primary_key.fingerprint(), "56c65c513a0d1b9cff532d784c073ae0c8445c0c");

//         match key.primary_key {
//             PrimaryKey::PublicKey(PublicKey::RSA {
//                                       version,
//                                       algorithm,
//                                       e,
//                                       n,
//                                   }) => {
//                 assert_eq!(version, KeyVersion::V4);
//                 assert_eq!(algorithm, PublicKeyAlgorithm::RSA);
//                 assert_eq!(n.len(), 512);
//                 assert_eq!(e, vec![1, 0, 1]);
//             }
//             _ => panic!("wrong key returned: {:?}", key.primary_key),
//         }

//         let mut sig1 = Signature::new(
//             SignatureVersion::V4,
//             SignatureType::CertPositive,
//             PublicKeyAlgorithm::RSA,
//             HashAlgorithm::SHA1,
//         );

//         let key_flags = vec![3];
//         let p_sym_algs = vec![
//             SymmetricKeyAlgorithm::AES256,
//             SymmetricKeyAlgorithm::AES192,
//             SymmetricKeyAlgorithm::AES128,
//             SymmetricKeyAlgorithm::CAST5,
//             SymmetricKeyAlgorithm::TripleDES,
//         ];
//         let p_com_algs = vec![
//             CompressionAlgorithm::ZLIB,
//             CompressionAlgorithm::BZip2,
//             CompressionAlgorithm::ZIP,
//         ];
//         let p_hash_algs = vec![
//             HashAlgorithm::SHA256,
//             HashAlgorithm::SHA1,
//             HashAlgorithm::SHA384,
//             HashAlgorithm::SHA512,
//             HashAlgorithm::SHA224,
//         ];
//         let issuer = Subpacket::Issuer([0x4C, 0x07, 0x3A, 0xE0, 0xC8, 0x44, 0x5C, 0x0C]);

//         sig1.created = Some(
//             DateTime::parse_from_rfc3339("2014-06-06T15:57:41Z")
//                 .expect("failed to parse static time")
//                 .with_timezone(&Utc),
//         );

//         sig1.key_flags = key_flags.clone();
//         sig1.preferred_symmetric_algs = p_sym_algs.clone();
//         sig1.preferred_compression_algs = p_com_algs.clone();
//         sig1.preferred_hash_algs = p_hash_algs.clone();

//         sig1.key_server_prefs = vec![128];
//         sig1.features = vec![1];

//         sig1.unhashed_subpackets.push(issuer.clone());

//         let u1 = User::new("john doe (test) <johndoe@example.com>", vec![sig1]);

//         let mut sig2 = Signature::new(
//             SignatureVersion::V4,
//             SignatureType::CertPositive,
//             PublicKeyAlgorithm::RSA,
//             HashAlgorithm::SHA1,
//         );

//         sig2.created = Some(
//             DateTime::parse_from_rfc3339("2014-06-06T16:21:46Z")
//                 .expect("failed to parse static time")
//                 .with_timezone(&Utc),
//         );

//         sig2.key_flags = key_flags.clone();
//         sig2.preferred_symmetric_algs = p_sym_algs.clone();
//         sig2.preferred_compression_algs = p_com_algs.clone();
//         sig2.preferred_hash_algs = p_hash_algs.clone();

//         sig2.key_server_prefs = vec![128];
//         sig2.features = vec![1];

//         sig2.unhashed_subpackets.push(issuer.clone());

//         let u2 = User::new("john doe <johndoe@seconddomain.com>", vec![sig2]);

//         assert_eq!(key.users.len(), 2);
//         assert_eq!(key.users[0], u1);
//         assert_eq!(key.users[1], u2);
//         assert_eq!(key.user_attributes.len(), 1);
//         let ua = &key.user_attributes[0];
//         match &ua.attr {
//             &UserAttributeType::Image(ref v) => {
//                 assert_eq!(v.len(), 1156);
//             }
//         }

//         let mut sig3 = Signature::new(
//             SignatureVersion::V4,
//             SignatureType::CertPositive,
//             PublicKeyAlgorithm::RSA,
//             HashAlgorithm::SHA1,
//         );

//         sig3.key_flags = key_flags.clone();
//         sig3.preferred_symmetric_algs = p_sym_algs.clone();
//         sig3.preferred_compression_algs = p_com_algs.clone();
//         sig3.preferred_hash_algs = p_hash_algs.clone();

//         sig3.key_server_prefs = vec![128];
//         sig3.features = vec![1];

//         sig3.unhashed_subpackets.push(issuer.clone());

//         sig3.created = Some(
//             DateTime::parse_from_rfc3339("2014-06-06T16:05:43Z")
//                 .expect("failed to parse static time")
//                 .with_timezone(&Utc),
//         );

//         assert_eq!(ua.signatures, vec![sig3]);
//     }
// }
