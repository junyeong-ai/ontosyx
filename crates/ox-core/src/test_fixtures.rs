use crate::ontology_ir::{
    Cardinality, ConstraintDef, EdgeTypeDef, IndexDef, NodeConstraint, NodeTypeDef, OntologyIR,
    PropertyDef,
};
use crate::types::PropertyType;

/// Create a simple non-nullable string property with the given id and name.
pub fn property(id: &str, name: &str) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: name.to_string(),
        property_type: PropertyType::String,
        nullable: false,
        default_value: None,
        description: None,
        classification: None,
    }
}

/// Build a standard test ontology with Person, Company, WORKS_AT edge, and one index.
pub fn test_ontology() -> OntologyIR {
    OntologyIR::new(
        "ont-1".to_string(),
        "Test Ontology".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "n1".into(),
                label: "Person".to_string(),
                description: Some("A person".to_string()),
                source_table: None,
                properties: vec![property("p1", "name"), property("p2", "age")],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p1".into()],
                    },
                }],
                ..Default::default()
            },
            NodeTypeDef {
                id: "n2".into(),
                label: "Company".to_string(),
                description: None,
                source_table: None,
                properties: vec![property("p3", "company_name")],
                constraints: vec![],
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e1".into(),
            label: "WORKS_AT".to_string(),
            description: None,
            source_node_id: "n1".into(),
            target_node_id: "n2".into(),
            properties: vec![property("ep1", "since")],
            cardinality: Cardinality::ManyToOne,
            ..Default::default()
        }],
        vec![IndexDef::Single {
            id: "idx1".to_string(),
            node_id: "n1".into(),
            property_id: "p1".into(),
        }],
    )
}

/// Compare ontologies structurally via serialization.
///
/// Returns `false` on any serialization failure. In practice this is
/// unreachable for well-formed `OntologyIR` values, but we avoid `.unwrap()`
/// so downstream crates can enable `clippy::unwrap_used = "deny"`.
pub fn ontologies_equal(a: &OntologyIR, b: &OntologyIR) -> bool {
    let (Ok(a_json), Ok(b_json)) = (serde_json::to_string(a), serde_json::to_string(b)) else {
        return false;
    };
    a_json == b_json
}

// ---------------------------------------------------------------------------
// Korean e-commerce golden fixture (Phase 0.5)
//
// This is the MVP baseline ontology used across unit and integration tests
// to validate that the entire platform handles Korean labels, property
// names, and relationship types correctly — from IR construction, through
// Cypher compilation, schema DDL generation, query compilation, and LLM
// context formatting.
//
// Stable internal identifiers are ASCII kebab-case (they are opaque to
// users and must be deterministic across runs). User-facing labels and
// property names are Korean.
// ---------------------------------------------------------------------------

fn prop(id: &str, name: &str, property_type: PropertyType, nullable: bool) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: name.to_string(),
        property_type,
        nullable,
        default_value: None,
        description: None,
        classification: None,
    }
}

fn unique_on(constraint_id: &str, property_id: &str) -> ConstraintDef {
    ConstraintDef {
        id: constraint_id.into(),
        constraint: NodeConstraint::Unique {
            property_ids: vec![property_id.into()],
        },
    }
}

/// Korean e-commerce ontology (고객 / 주문 / 상품 / 카테고리 / 리뷰 / 배송 / 결제수단).
///
/// 7 node types, 6 edge types. Used as the canonical Korean-domain fixture
/// for compilation, query generation, and LLM context round-trip tests.
pub fn korean_ecommerce_ontology() -> OntologyIR {
    let node_types = vec![
        NodeTypeDef {
            id: "n-customer".into(),
            label: "고객".to_string(),
            description: Some("구매 활동의 주체".to_string()),
            source_table: None,
            properties: vec![
                prop("p-customer-id", "고객번호", PropertyType::String, false),
                prop("p-customer-name", "이름", PropertyType::String, false),
                prop("p-customer-email", "이메일", PropertyType::String, true),
                prop("p-customer-joined", "가입일시", PropertyType::DateTime, false),
            ],
            constraints: vec![unique_on("c-customer-id", "p-customer-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-order".into(),
            label: "주문".to_string(),
            description: Some("고객이 발행한 주문 건".to_string()),
            source_table: None,
            properties: vec![
                prop("p-order-id", "주문번호", PropertyType::String, false),
                prop("p-order-placed", "주문일시", PropertyType::DateTime, false),
                prop("p-order-total", "총액", PropertyType::Int, false),
            ],
            constraints: vec![unique_on("c-order-id", "p-order-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-product".into(),
            label: "상품".to_string(),
            description: Some("판매 가능한 상품 SKU".to_string()),
            source_table: None,
            properties: vec![
                prop("p-product-id", "상품번호", PropertyType::String, false),
                prop("p-product-name", "상품명", PropertyType::String, false),
                prop("p-product-price", "가격", PropertyType::Int, false),
                prop("p-product-stock", "재고", PropertyType::Int, false),
            ],
            constraints: vec![unique_on("c-product-id", "p-product-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-category".into(),
            label: "카테고리".to_string(),
            description: Some("상품 분류".to_string()),
            source_table: None,
            properties: vec![
                prop("p-category-id", "카테고리번호", PropertyType::String, false),
                prop("p-category-name", "카테고리명", PropertyType::String, false),
            ],
            constraints: vec![unique_on("c-category-id", "p-category-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-review".into(),
            label: "리뷰".to_string(),
            description: Some("고객이 작성한 상품 리뷰".to_string()),
            source_table: None,
            properties: vec![
                prop("p-review-id", "리뷰번호", PropertyType::String, false),
                prop("p-review-rating", "평점", PropertyType::Int, false),
                prop("p-review-content", "내용", PropertyType::String, true),
                prop("p-review-written", "작성일시", PropertyType::DateTime, false),
            ],
            constraints: vec![unique_on("c-review-id", "p-review-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-shipment".into(),
            label: "배송".to_string(),
            description: Some("주문의 배송 상태".to_string()),
            source_table: None,
            properties: vec![
                prop("p-shipment-id", "배송번호", PropertyType::String, false),
                prop("p-shipment-status", "배송상태", PropertyType::String, false),
                prop("p-shipment-date", "배송일시", PropertyType::DateTime, true),
            ],
            constraints: vec![unique_on("c-shipment-id", "p-shipment-id")],
            ..Default::default()
        },
        NodeTypeDef {
            id: "n-payment".into(),
            label: "결제수단".to_string(),
            description: Some("주문에 사용된 결제 수단".to_string()),
            source_table: None,
            properties: vec![
                prop("p-payment-id", "결제번호", PropertyType::String, false),
                prop("p-payment-method", "결제방법", PropertyType::String, false),
                prop("p-payment-amount", "결제금액", PropertyType::Int, false),
            ],
            constraints: vec![unique_on("c-payment-id", "p-payment-id")],
            ..Default::default()
        },
    ];

    let edge_types = vec![
        EdgeTypeDef {
            id: "e-ordered".into(),
            label: "주문함".to_string(),
            description: Some("고객이 주문을 발행".to_string()),
            source_node_id: "n-customer".into(),
            target_node_id: "n-order".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
            ..Default::default()
        },
        EdgeTypeDef {
            id: "e-contains".into(),
            label: "포함".to_string(),
            description: Some("주문에 포함된 상품".to_string()),
            source_node_id: "n-order".into(),
            target_node_id: "n-product".into(),
            properties: vec![prop("p-contains-qty", "수량", PropertyType::Int, false)],
            cardinality: Cardinality::ManyToMany,
            ..Default::default()
        },
        EdgeTypeDef {
            id: "e-belongs".into(),
            label: "속함".to_string(),
            description: Some("상품이 속한 카테고리".to_string()),
            source_node_id: "n-product".into(),
            target_node_id: "n-category".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToOne,
            ..Default::default()
        },
        EdgeTypeDef {
            id: "e-wrote".into(),
            label: "작성함".to_string(),
            description: Some("고객이 리뷰 작성".to_string()),
            source_node_id: "n-customer".into(),
            target_node_id: "n-review".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
            ..Default::default()
        },
        EdgeTypeDef {
            id: "e-shipped".into(),
            label: "배송됨".to_string(),
            description: Some("주문의 배송 연결".to_string()),
            source_node_id: "n-order".into(),
            target_node_id: "n-shipment".into(),
            properties: vec![],
            cardinality: Cardinality::OneToOne,
            ..Default::default()
        },
        EdgeTypeDef {
            id: "e-paid".into(),
            label: "결제함".to_string(),
            description: Some("주문의 결제 연결".to_string()),
            source_node_id: "n-order".into(),
            target_node_id: "n-payment".into(),
            properties: vec![],
            cardinality: Cardinality::OneToOne,
            ..Default::default()
        },
    ];

    let indexes = vec![IndexDef::Single {
        id: "idx-product-name".to_string(),
        node_id: "n-product".into(),
        property_id: "p-product-name".into(),
    }];

    OntologyIR::new(
        "korean-ecommerce".to_string(),
        "한글 전자상거래 온톨로지".to_string(),
        Some("Phase 0 Korean MVP golden fixture".to_string()),
        1,
        node_types,
        edge_types,
        indexes,
    )
}
