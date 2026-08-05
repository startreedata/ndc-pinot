use ndc_models::{
    Capabilities, CapabilitiesResponse, ExistsCapabilities, LeafCapability, MutationCapabilities,
    NestedFieldCapabilities, QueryCapabilities,
};

/// The capabilities this connector declares.
///
/// Notably **no `relationships`**: Apache Pinot's SQL engine has limited,
/// non-correlated JOIN support with no equivalent of the correlated
/// subqueries a single-statement join resolution strategy would need to
/// implement NDC relationships in one statement. Rather than emulate
/// relationships with fragile N+1 query stitching, this connector declares
/// no relationship support in v1 and leaves cross-collection joins to the
/// Hasura engine (which can still perform them as "remote joins" across
/// separate connector queries).
///
/// No mutations either - Pinot is a read-only OLAP query engine from this
/// connector's perspective; writes happen via ingestion, not SQL.
pub fn capabilities() -> Capabilities {
    Capabilities {
        query: QueryCapabilities {
            aggregates: Some(LeafCapability {}),
            variables: Some(LeafCapability {}),
            explain: Some(LeafCapability {}),
            nested_fields: NestedFieldCapabilities {
                filter_by: None,
                order_by: None,
                aggregates: None,
            },
            exists: ExistsCapabilities {
                nested_collections: None,
            },
        },
        mutation: MutationCapabilities {
            transactional: None,
            explain: None,
        },
        relationships: None,
    }
}

pub fn capabilities_response() -> CapabilitiesResponse {
    CapabilitiesResponse {
        version: ndc_models::VERSION.into(),
        capabilities: capabilities(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_relationships_declared() {
        assert!(capabilities().relationships.is_none());
    }
}
