use nom::IResult;

use packet::{Packet, packet_parser};
use packet::types::Key;
use armor;

named!(packets_parser<Vec<Packet>>, many1!(packet_parser));

fn parse_key_raw<'a>(input: &'a [u8]) -> IResult<&'a [u8], armor::Block<'a>> {
    armor::parse(input).map(|(typ, headers, body)| {
        // TODO: Proper error handling
        let (_, packets) = packets_parser(body.as_slice()).unwrap();
        armor::Block {
            typ: typ,
            headers: headers,
            packets: packets,
        }
    })
}

// TODO: change to regular result
pub fn parse_key(input: &[u8]) -> IResult<&[u8], Key> {
    let block = parse_key_raw(input).to_result().expect("Invalid input");

    Key::from_block(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use packet::types::{PublicKey, PrimaryKey, KeyVersion, PublicKeyAlgorithm};

    #[test]
    fn test_parse() {
        let raw = include_bytes!("../tests/opengpg-interop/testcases/keys/gnupg-v1-003.asc");
        let (_, key) = parse_key(raw).unwrap();

        // assert_eq!(key.primary_key.fingerprint(), "56c65c513a0d1b9cff532d784c073ae0c8445c0c");

        match key.primary_key {
            PrimaryKey::PublicKey(PublicKey::RSAPublicKey {
                                      version,
                                      algorithm,
                                      e,
                                      n,
                                  }) => {
                assert_eq!(version, KeyVersion::V4);
                assert_eq!(algorithm, PublicKeyAlgorithm::RSA);
                assert_eq!(n.len(), 512);
                assert_eq!(e, vec![1, 0, 1]);
            }
            _ => panic!("wrong key returned: {:?}", key.primary_key),
        }
    }
}