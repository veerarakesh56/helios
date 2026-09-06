//! Per-resource availability models.
//!
//! Each AWS resource kind we support has a canonical failure footprint. An EC2 instance
//! lives in exactly one AZ; an RDS with `multi_az = true` spans two; an S3 bucket is
//! regional. These are hand-authored, not inferred, because the correctness of the whole
//! simulator depends on them. Contributions welcome — see docs/availability-models.md.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::Range;

pub type Region = String;
pub type Az = String;

/// How a resource survives failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvailabilityModel {
    /// Lives in exactly one AZ. Fails if that AZ fails.
    SingleAz { az: Az },
    /// Spans multiple AZs with a defined failover time window.
    MultiAz {
        azs: Vec<Az>,
        #[serde(with = "range_ser")]
        failover_seconds: Range<u32>,
    },
    /// Regional control plane. Fails only if the whole region fails.
    Regional { region: Region },
    /// Global edge (Route53, CloudFront). Only a full-provider outage affects it.
    GlobalEdge,
}

/// Infer the availability model for a Terraform resource from its type + attrs.
///
/// `default_region` is the LAST resort. The region is derived per resource by [`region_for`] —
/// an explicit `region`, the zone it declares, the first zone it spans, or the region field of an
/// ARN it carries — because `terraform show -json` does not carry the provider's region. Callers
/// should pass [`infer_region`] over the whole graph as the default, so the handful of resources
/// with no region signal at all land in the region everything else agrees on rather than in
/// `us-east-1`.
pub fn availability_for(tf_type: &str, attrs: &Value, default_region: &str) -> AvailabilityModel {
    match tf_type {
        "aws_vpc" => AvailabilityModel::Regional {
            region: region_of(attrs, default_region),
        },
        "aws_subnet" => AvailabilityModel::SingleAz {
            az: string_attr(attrs, "availability_zone")
                .unwrap_or_else(|| format!("{}a", region_for(attrs, default_region))),
        },
        "aws_instance" => AvailabilityModel::SingleAz {
            az: string_attr(attrs, "availability_zone")
                .unwrap_or_else(|| format!("{}a", region_for(attrs, default_region))),
        },
        "aws_lb" => AvailabilityModel::MultiAz {
            azs: azs_from_subnets(attrs),
            // ALB health-check + DNS propagation — fast.
            failover_seconds: 5..30,
        },
        "aws_db_instance" => {
            let multi_az = attrs
                .get("multi_az")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if multi_az {
                AvailabilityModel::MultiAz {
                    // Terraform does not say WHICH zones a multi-AZ RDS occupies, so the pair is a
                    // guess — but it must be a guess inside the resource's OWN region, which is
                    // derivable. Using the global default here silently placed every non-us-east-1
                    // database in us-east-1, so it survived its real region going down.
                    azs: {
                        let region = region_for(attrs, default_region);
                        vec![format!("{region}a"), format!("{region}b")]
                    },
                    // RDS published failover window.
                    failover_seconds: 30..120,
                }
            } else {
                AvailabilityModel::SingleAz {
                    az: string_attr(attrs, "availability_zone")
                        .unwrap_or_else(|| format!("{}a", region_for(attrs, default_region))),
                }
            }
        }
        "aws_elasticache_cluster" => {
            // Replication is configured via a separate aws_elasticache_replication_group
            // resource which we don't parse in v0.1. Treat plain clusters as SingleAz.
            AvailabilityModel::SingleAz {
                az: string_attr(attrs, "availability_zone")
                    .unwrap_or_else(|| format!("{default_region}a")),
            }
        }
        "aws_lambda_function" => {
            // Lambda is a regional service. If vpc_config.subnet_ids spans multiple AZs,
            // cold-starts can still run in any of them, but the failure surface is regional.
            AvailabilityModel::Regional {
                region: region_of(attrs, default_region),
            }
        }
        "aws_s3_bucket" => AvailabilityModel::Regional {
            region: region_of(attrs, default_region),
        },
        // Edge-ish services we'll add in v0.2+
        _ => AvailabilityModel::Regional {
            region: region_for(attrs, default_region),
        },
    }
}

/// Strip the trailing zone letter from an AZ id: `eu-west-2a` -> `eu-west-2`.
pub fn region_of_az(az: &str) -> Region {
    if az
        .as_bytes()
        .last()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        az[..az.len() - 1].to_string()
    } else {
        az.to_string()
    }
}

/// The region field of an ARN: `arn:partition:service:REGION:account:resource`.
///
/// Empty for global services (IAM, S3 bucket ARNs, CloudFront), so an empty field is not a region.
fn region_from_arn(arn: &str) -> Option<Region> {
    let field = arn.split(':').nth(3)?;
    if field.is_empty() || !field.contains('-') {
        return None;
    }
    Some(field.to_string())
}

/// Work out which region a resource is in, from whatever Terraform actually emitted.
///
/// `terraform show -json` does not carry the provider's region, so a hard-coded default put every
/// resource without an explicit `region` attribute in us-east-1 — which silently produced the wrong
/// answer for any other estate. Prefer, in order: an explicit region, the zone it declares, the
/// first zone it spans, the region field of any ARN it carries, then the caller's default.
pub fn region_for(attrs: &Value, default_region: &str) -> Region {
    if let Some(r) = string_attr(attrs, "region") {
        return r;
    }
    if let Some(az) = string_attr(attrs, "availability_zone") {
        return region_of_az(&az);
    }
    if let Some(az) = azs_from_subnets(attrs).first() {
        return region_of_az(az);
    }
    // Any attribute that looks like an ARN. `arn` first, then anything else ending in `_arn`, so
    // the resource's own ARN wins over a reference to something else.
    if let Some(arn) = string_attr(attrs, "arn").and_then(|a| region_from_arn(&a)) {
        return arn;
    }
    if let Some(map) = attrs.as_object() {
        let mut keys: Vec<&String> = map.keys().filter(|k| k.ends_with("_arn")).collect();
        keys.sort();
        for k in keys {
            if let Some(r) = map[k].as_str().and_then(region_from_arn) {
                return r;
            }
        }
    }
    default_region.to_string()
}

/// The region the graph as a whole is in: the one the most resources agree on.
///
/// Some resources carry no region signal at all (in the shipped fixture, the Lambda), so a
/// per-resource rule is not enough on its own. Ties break alphabetically so the answer is stable.
pub fn infer_region<'a>(all_attrs: impl Iterator<Item = &'a Value>) -> Option<Region> {
    let mut counts: std::collections::BTreeMap<Region, usize> = Default::default();
    for attrs in all_attrs {
        // Sentinel that cannot be a real region, so "no signal" never wins the vote.
        let r = region_for(attrs, "\0none");
        if r != "\0none" {
            *counts.entry(r).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(r, _)| r)
}

fn region_of(attrs: &Value, default_region: &str) -> Region {
    region_for(attrs, default_region)
}

fn string_attr(attrs: &Value, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn azs_from_subnets(attrs: &Value) -> Vec<Az> {
    if let Some(azs) = attrs.get("availability_zones").and_then(|v| v.as_array()) {
        return azs
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    Vec::new()
}

mod range_ser {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ops::Range;

    #[derive(Serialize, Deserialize)]
    struct Pair {
        start: u32,
        end: u32,
    }

    pub fn serialize<S: Serializer>(r: &Range<u32>, s: S) -> Result<S::Ok, S::Error> {
        Pair {
            start: r.start,
            end: r.end,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Range<u32>, D::Error> {
        let p = Pair::deserialize(d)?;
        Ok(p.start..p.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REGION: &str = "us-east-1";

    #[test]
    fn vpc_is_regional() {
        let m = availability_for("aws_vpc", &json!({}), REGION);
        assert_eq!(
            m,
            AvailabilityModel::Regional {
                region: REGION.into()
            }
        );
    }

    #[test]
    fn subnet_is_single_az() {
        let m = availability_for(
            "aws_subnet",
            &json!({"availability_zone": "us-east-1b"}),
            REGION,
        );
        assert_eq!(
            m,
            AvailabilityModel::SingleAz {
                az: "us-east-1b".into()
            }
        );
    }

    #[test]
    fn rds_multi_az() {
        let m = availability_for("aws_db_instance", &json!({"multi_az": true}), REGION);
        assert!(matches!(m, AvailabilityModel::MultiAz { .. }));
    }

    #[test]
    fn rds_single_az_when_multi_az_false() {
        let m = availability_for(
            "aws_db_instance",
            &json!({"multi_az": false, "availability_zone": "us-east-1c"}),
            REGION,
        );
        assert_eq!(
            m,
            AvailabilityModel::SingleAz {
                az: "us-east-1c".into()
            }
        );
    }

    #[test]
    fn elasticache_single_az_by_default() {
        let m = availability_for(
            "aws_elasticache_cluster",
            &json!({"availability_zone": "us-east-1a"}),
            REGION,
        );
        assert_eq!(
            m,
            AvailabilityModel::SingleAz {
                az: "us-east-1a".into()
            }
        );
    }

    #[test]
    fn alb_is_multi_az() {
        let m = availability_for(
            "aws_lb",
            &json!({"availability_zones": ["us-east-1a", "us-east-1b"]}),
            REGION,
        );
        match m {
            AvailabilityModel::MultiAz { azs, .. } => {
                assert_eq!(
                    azs,
                    vec!["us-east-1a".to_string(), "us-east-1b".to_string()]
                );
            }
            _ => panic!("expected MultiAz"),
        }
    }

    #[test]
    fn s3_is_regional() {
        let m = availability_for("aws_s3_bucket", &json!({"region": "us-east-2"}), REGION);
        assert_eq!(
            m,
            AvailabilityModel::Regional {
                region: "us-east-2".into()
            }
        );
    }

    #[test]
    fn lambda_is_regional() {
        let m = availability_for("aws_lambda_function", &json!({}), REGION);
        assert_eq!(
            m,
            AvailabilityModel::Regional {
                region: REGION.into()
            }
        );
    }

    #[test]
    fn ec2_instance_is_single_az() {
        let m = availability_for(
            "aws_instance",
            &json!({"availability_zone": "us-east-1a"}),
            REGION,
        );
        assert_eq!(
            m,
            AvailabilityModel::SingleAz {
                az: "us-east-1a".into()
            }
        );
    }

    #[test]
    fn models_roundtrip_json() {
        let m = AvailabilityModel::MultiAz {
            azs: vec!["us-east-1a".into(), "us-east-1b".into()],
            failover_seconds: 30..90,
        };
        let encoded = serde_json::to_string(&m).unwrap();
        let decoded: AvailabilityModel = serde_json::from_str(&encoded).unwrap();
        assert_eq!(m, decoded);
    }
}
