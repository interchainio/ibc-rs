use ibc_proto::ics23::ProofSpec as ProtoProofSpec;

/// An array of proof specifications.
///
/// This type encapsulates different types of proof specifications, mostly predefined, e.g., for
/// Cosmos-SDK.
/// Additionally, this type also aids in the conversion from `ProofSpec` types from crate `ics23`
/// into proof specifications as represented in the `ibc_proto` type; see the
/// `From` trait(s) below.
pub struct ProofSpecs {
    specs: Vec<ics23::ProofSpec>,
}

impl ProofSpecs {
    /// Returns the specification for Cosmos-SDK proofs
    pub fn cosmos() -> Self {
        Self {
            specs: vec![
                ics23::iavl_spec(),       // Format of proofs-iavl (iavl merkle proofs)
                ics23::tendermint_spec(), // Format of proofs-tendermint (crypto/ merkle SimpleProof)
            ],
        }
    }
}

/// Converts from the domain type (which is represented as a vector of `ics23::ProofSpec`
/// to the corresponding proto type (vector of `ibc_proto::ProofSpec`).
/// TODO: fix with <https://github.com/informalsystems/ibc-rs/issues/853>
impl From<ProofSpecs> for Vec<ProtoProofSpec> {
    fn from(domain_specs: ProofSpecs) -> Self {
        use ibc_proto::ics23::InnerSpec as ProtoInnerSpec;
        use ibc_proto::ics23::LeafOp as ProtoLeafOp;

        fn to_proto_leaf(leaf: ics23::LeafOp) -> ProtoLeafOp {
            ProtoLeafOp {
                hash: leaf.hash,
                length: leaf.length,
                prefix: leaf.prefix,
                prehash_key: leaf.prehash_key,
                prehash_value: leaf.prehash_value,
            }
        }

        fn to_proto_inner(inner: ics23::InnerSpec) -> ProtoInnerSpec {
            ProtoInnerSpec {
                child_order: inner.child_order,
                child_size: inner.child_size,
                min_prefix_length: inner.min_prefix_length,
                max_prefix_length: inner.max_prefix_length,
                empty_child: inner.empty_child,
                hash: inner.hash,
            }
        }

        domain_specs
            .specs
            .into_iter()
            .map(|spec| ProtoProofSpec {
                leaf_spec: spec.leaf_spec.map(to_proto_leaf),
                inner_spec: spec.inner_spec.map(to_proto_inner),
                max_depth: spec.max_depth,
                min_depth: spec.min_depth,
            })
            .collect()
    }
}
