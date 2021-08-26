use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::connection::v1::Version as RawVersion;

use crate::ics03_connection::error::Error;

/// Stores the identifier and the features supported by a version
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    /// unique version identifier
    identifier: String,
    /// list of features compatible with the specified identifier
    features: Vec<String>,
}

impl Version {
    /// Checks whether or not the given feature is supported in this versin
    pub fn is_supported_feature(&self, feature: String) -> bool {
        self.features.contains(&feature)
    }
}

impl Protobuf<RawVersion> for Version {}

impl TryFrom<RawVersion> for Version {
    type Error = Error;
    fn try_from(value: RawVersion) -> Result<Self, Self::Error> {
        if value.identifier.trim().is_empty() {
            return Err(Error::empty_versions());
        }
        for feature in value.features.iter() {
            if feature.trim().is_empty() {
                return Err(Error::empty_features());
            }
        }
        Ok(Version {
            identifier: value.identifier,
            features: value.features,
        })
    }
}

impl From<Version> for RawVersion {
    fn from(value: Version) -> Self {
        Self {
            identifier: value.identifier,
            features: value.features,
        }
    }
}

impl Default for Version {
    fn default() -> Self {
        Version {
            identifier: "1".to_string(),
            features: vec!["ORDER_ORDERED".to_string(), "ORDER_UNORDERED".to_string()],
        }
    }
}

/// Returns the lists of supported versions
pub fn get_compatible_versions() -> Vec<Version> {
    vec![Version::default()]
}

/// Selects a version from the intersection of locally supported and counterparty versions.
pub fn pick_version(
    supported_versions: Vec<Version>,
    counterparty_versions: Vec<Version>,
) -> Result<Version, Error> {
    let mut intersection: Vec<Version> = vec![];
    for s in supported_versions.iter() {
        for c in counterparty_versions.iter() {
            if c.identifier != s.identifier {
                continue;
            }
            for feature in c.features.iter() {
                if feature.trim().is_empty() {
                    return Err(Error::empty_features());
                }
            }
            intersection.append(&mut vec![s.clone()]);
        }
    }
    intersection.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    if intersection.is_empty() {
        return Err(Error::no_common_version());
    }
    Ok(intersection[0].clone())
}

#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};
    use test_env_log::test;

    use ibc_proto::ibc::core::connection::v1::Version as RawVersion;

    use crate::ics03_connection::error::Error;
    use crate::ics03_connection::version::{get_compatible_versions, pick_version, Version};

    fn good_versions() -> Vec<RawVersion> {
        vec![
            Version::default().into(),
            RawVersion {
                identifier: "2".to_string(),
                features: vec!["ORDER_RANDOM".to_string(), "ORDER_UNORDERED".to_string()],
            },
        ]
        .into_iter()
        .collect()
    }

    fn bad_versions_identifier() -> Vec<RawVersion> {
        vec![RawVersion {
            identifier: "".to_string(),
            features: vec!["ORDER_RANDOM".to_string(), "ORDER_UNORDERED".to_string()],
        }]
        .into_iter()
        .collect()
    }

    fn bad_versions_features() -> Vec<RawVersion> {
        vec![RawVersion {
            identifier: "2".to_string(),
            features: vec!["".to_string()],
        }]
        .into_iter()
        .collect()
    }

    fn overlapping() -> (Vec<Version>, Vec<Version>, Version) {
        (
            vec![
                Version::default(),
                Version {
                    identifier: "3".to_string(),
                    features: vec![],
                },
                Version {
                    identifier: "4".to_string(),
                    features: vec![],
                },
            ]
            .into_iter()
            .collect(),
            vec![
                Version {
                    identifier: "2".to_string(),
                    features: vec![],
                },
                Version {
                    identifier: "4".to_string(),
                    features: vec![],
                },
                Version {
                    identifier: "3".to_string(),
                    features: vec![],
                },
            ]
            .into_iter()
            .collect(),
            // Should pick version 3 as it's the lowest of the intersection {3, 4}
            Version {
                identifier: "3".to_string(),
                features: vec![],
            },
        )
    }

    fn disjoint() -> (Vec<Version>, Vec<Version>) {
        (
            vec![Version {
                identifier: "1".to_string(),
                features: vec![],
            }]
            .into_iter()
            .collect(),
            vec![Version {
                identifier: "2".to_string(),
                features: vec![],
            }]
            .into_iter()
            .collect(),
        )
    }

    #[test]
    fn verify() {
        struct Test {
            name: String,
            versions: Vec<RawVersion>,
            want_pass: bool,
        }
        let tests: Vec<Test> = vec![
            Test {
                name: "Compatible versions".to_string(),
                versions: vec![Version::default().into()],
                want_pass: true,
            },
            Test {
                name: "Multiple versions".to_string(),
                versions: good_versions(),
                want_pass: true,
            },
            Test {
                name: "Bad version identifier".to_string(),
                versions: bad_versions_identifier(),
                want_pass: false,
            },
            Test {
                name: "Bad version feature".to_string(),
                versions: bad_versions_features(),
                want_pass: false,
            },
            Test {
                name: "Bad versions empty".to_string(),
                versions: vec![],
                want_pass: true,
            },
        ];

        for test in tests {
            let versions = test
                .versions
                .into_iter()
                .map(Version::try_from)
                .collect::<Result<Vec<_>, _>>();

            assert_eq!(
                test.want_pass,
                versions.is_ok(),
                "Validate versions failed for test {} with error {:?}",
                test.name,
                versions.err(),
            );
        }
    }
    #[test]
    fn pick() {
        struct Test {
            name: String,
            supported: Vec<Version>,
            counterparty: Vec<Version>,
            picked: Result<Version, Error>,
            want_pass: bool,
        }
        let tests: Vec<Test> = vec![
            Test {
                name: "Compatible versions".to_string(),
                supported: get_compatible_versions(),
                counterparty: get_compatible_versions(),
                picked: Ok(Version::default()),
                want_pass: true,
            },
            Test {
                name: "Overlapping versions".to_string(),
                supported: overlapping().0,
                counterparty: overlapping().1,
                picked: Ok(overlapping().2),
                want_pass: true,
            },
            Test {
                name: "Disjoint versions".to_string(),
                supported: disjoint().0,
                counterparty: disjoint().1,
                picked: Err(Error::no_common_version()),
                want_pass: false,
            },
        ];

        for test in tests {
            let version = pick_version(test.supported, test.counterparty);

            assert_eq!(
                test.want_pass,
                version.is_ok(),
                "Validate versions failed for test {}",
                test.name,
            );

            if test.want_pass {
                assert_eq!(version.unwrap(), test.picked.unwrap());
            }
        }
    }
    #[test]
    fn serialize() {
        let def = Version::default();
        let def_raw: RawVersion = def.clone().into();
        let def_back = def_raw.try_into().unwrap();
        assert_eq!(def, def_back);
    }
}
