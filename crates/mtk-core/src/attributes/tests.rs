//! Unit tests for attributes data representations and schemas.

use super::schema::*;
use super::types::*;

#[test]
fn test_attribute_data_as_bytes() {
    let f = AttributeData::Float(vec![1.0, 2.0]);
    let bytes = f.as_bytes().unwrap();
    assert_eq!(bytes.len(), 8);

    let f3 = AttributeData::Float3(vec![[1.0, 2.0, 3.0]]);
    assert_eq!(f3.as_bytes().unwrap().len(), 12);

    let b = AttributeData::Bool(vec![true, false, true]);
    let bytes = b.as_bytes().unwrap();
    assert_eq!(bytes, &[1, 0, 1]);

    let s = AttributeData::String(vec!["hello".to_string()]);
    assert!(s.as_bytes().is_none());
}

#[test]
fn test_canonical_attribute_specs_are_unique_and_typed() {
    let mut names = std::collections::HashSet::new();
    for spec in MESH_ATTRIBUTE_SPECS {
        assert!(
            spec.name.starts_with("mtk_"),
            "unexpected name: {}",
            spec.name
        );
        assert!(
            names.insert(spec.name),
            "duplicate canonical attribute: {}",
            spec.name
        );
        assert!(!spec.data_type.is_empty());
        assert_ne!(spec.role, AttributeRole::Retired);
    }
}

#[test]
fn test_retired_attributes_are_disjoint_from_active_schema() {
    let active: std::collections::HashSet<&str> =
        MESH_ATTRIBUTE_SPECS.iter().map(|spec| spec.name).collect();
    let mut retired = std::collections::HashSet::new();
    for name in RETIRED_MESH_ATTRIBUTES {
        assert!(retired.insert(*name), "duplicate retired attribute: {name}");
        assert!(
            !active.contains(name),
            "attribute {name} is both active and retired"
        );
    }
}

#[test]
fn test_attribute_role_strings_are_stable() {
    assert_eq!(AttributeRole::Material.as_str(), "material");
    assert_eq!(AttributeRole::Metadata.as_str(), "metadata");
    assert_eq!(AttributeRole::Retired.as_str(), "retired");
}
