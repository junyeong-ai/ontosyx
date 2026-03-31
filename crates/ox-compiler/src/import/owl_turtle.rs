use std::collections::{BTreeMap, HashMap, HashSet};

use rio_api::model::{Literal, NamedNode, Subject, Term};
use rio_api::parser::TriplesParser;
use rio_turtle::TurtleParser;
use tracing::warn;

use ox_core::error::{OxError, OxResult};
use ox_core::ontology_input::{
    InputEdgeTypeDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef, OntologyInputIR,
    normalize,
};
use ox_core::ontology_ir::{Cardinality, OntologyIR};
use ox_core::types::PropertyType;

// ---------------------------------------------------------------------------
// Well-known IRI constants
// ---------------------------------------------------------------------------

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#int";
const XSD_LONG: &str = "http://www.w3.org/2001/XMLSchema#long";
const XSD_SHORT: &str = "http://www.w3.org/2001/XMLSchema#short";
const XSD_BYTE: &str = "http://www.w3.org/2001/XMLSchema#byte";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_POSITIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#positiveInteger";
const XSD_UNSIGNED_INT: &str = "http://www.w3.org/2001/XMLSchema#unsignedInt";
const XSD_UNSIGNED_LONG: &str = "http://www.w3.org/2001/XMLSchema#unsignedLong";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_DATE: &str = "http://www.w3.org/2001/XMLSchema#date";
const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const XSD_DURATION: &str = "http://www.w3.org/2001/XMLSchema#duration";
const XSD_BASE64_BINARY: &str = "http://www.w3.org/2001/XMLSchema#base64Binary";
const XSD_HEX_BINARY: &str = "http://www.w3.org/2001/XMLSchema#hexBinary";

// ---------------------------------------------------------------------------
// Internal triple collection
// ---------------------------------------------------------------------------

/// An owned triple extracted from parsed Turtle.
#[derive(Debug, Clone)]
struct OwnedTriple {
    subject: String,
    predicate: String,
    object: OwnedTerm,
}

#[derive(Debug, Clone)]
enum OwnedTerm {
    Iri(String),
    Literal {
        value: String,
        /// Retained for future use (e.g., inferring property types from literal values).
        #[allow(dead_code)]
        datatype: String,
    },
    Blank(String),
}

impl OwnedTerm {
    fn as_iri(&self) -> Option<&str> {
        match self {
            OwnedTerm::Iri(s) => Some(s),
            _ => None,
        }
    }

    fn as_literal_value(&self) -> Option<&str> {
        match self {
            OwnedTerm::Literal { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// Intermediate data for a blank-node restriction.
#[derive(Debug, Default)]
struct RestrictionInfo {
    on_property: Option<String>,
    max_cardinality: Option<u32>,
    min_cardinality: Option<u32>,
}

/// Intermediate data for a datatype property.
#[derive(Debug)]
struct DatatypePropInfo {
    iri: String,
    label: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    range: Option<String>,
    is_functional: bool,
}

/// Intermediate data for an object property.
#[derive(Debug)]
struct ObjectPropInfo {
    iri: String,
    label: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    range: Option<String>,
}

/// Intermediate data for a class.
#[derive(Debug)]
struct ClassInfo {
    #[allow(dead_code)]
    iri: String,
    label: Option<String>,
    description: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an OWL ontology in Turtle format into an OntologyIR.
///
/// Supported OWL 2 RL subset:
/// - `owl:Class` -> NodeTypeDef
/// - `owl:DatatypeProperty` -> PropertyDef on the domain class
/// - `owl:ObjectProperty` -> EdgeTypeDef
/// - `owl:FunctionalProperty` -> Unique constraint
/// - `owl:Restriction` (cardinality) -> Cardinality on edges
///
/// Unsupported constructs are silently skipped with a tracing warning.
pub fn parse_owl_turtle(turtle: &str) -> OxResult<OntologyIR> {
    // Phase 1: Parse into owned triples
    let triples = parse_triples(turtle)?;

    // Phase 2-6: Extract ontology structures
    let input = extract_ontology_input(&triples)?;

    // Use normalize() to assign proper UUIDs and resolve references
    let result = normalize(input).map_err(|errors| OxError::Compilation {
        message: format!("OWL import normalization failed: {}", errors.join("; ")),
    })?;

    Ok(result.ontology)
}

// ---------------------------------------------------------------------------
// Phase 1: Parse Turtle into owned triples
// ---------------------------------------------------------------------------

fn parse_triples(turtle: &str) -> OxResult<Vec<OwnedTriple>> {
    let mut triples = Vec::new();

    let reader = turtle.as_bytes();
    let mut parser = TurtleParser::new(reader, None);

    parser
        .parse_all(&mut |triple| -> Result<(), rio_turtle::TurtleError> {
            let subject = match triple.subject {
                Subject::NamedNode(NamedNode { iri }) => iri.to_string(),
                Subject::BlankNode(bnode) => format!("_:{}", bnode.id),
                Subject::Triple(_) => {
                    // RDF-star: skip quoted triples as subjects
                    return Ok(());
                }
            };

            let predicate = triple.predicate.iri.to_string();

            let object = match triple.object {
                Term::NamedNode(NamedNode { iri }) => OwnedTerm::Iri(iri.to_string()),
                Term::BlankNode(bnode) => OwnedTerm::Blank(format!("_:{}", bnode.id)),
                Term::Literal(lit) => match lit {
                    Literal::Simple { value } => OwnedTerm::Literal {
                        value: value.to_string(),
                        datatype: XSD_STRING.to_string(),
                    },
                    Literal::LanguageTaggedString { value, .. } => OwnedTerm::Literal {
                        value: value.to_string(),
                        datatype: XSD_STRING.to_string(),
                    },
                    Literal::Typed { value, datatype } => OwnedTerm::Literal {
                        value: value.to_string(),
                        datatype: datatype.iri.to_string(),
                    },
                },
                Term::Triple(_) => {
                    // RDF-star: skip
                    return Ok(());
                }
            };

            triples.push(OwnedTriple {
                subject,
                predicate,
                object,
            });
            Ok(())
        })
        .map_err(|e| OxError::Compilation {
            message: format!("Failed to parse Turtle: {e}"),
        })?;

    Ok(triples)
}

// ---------------------------------------------------------------------------
// Phase 2-6: Extract ontology structures from triples
// ---------------------------------------------------------------------------

fn extract_ontology_input(triples: &[OwnedTriple]) -> OxResult<OntologyInputIR> {
    // Build lookup indexes
    let mut subject_index: HashMap<&str, Vec<&OwnedTriple>> = HashMap::new();
    for t in triples {
        subject_index.entry(&t.subject).or_default().push(t);
    }

    // --- Ontology metadata ---
    let mut ontology_name: Option<String> = None;
    let mut ontology_description: Option<String> = None;

    // --- Classify subjects by rdf:type ---
    let mut class_iris: Vec<String> = Vec::new();
    let mut datatype_prop_iris: Vec<String> = Vec::new();
    let mut object_prop_iris: Vec<String> = Vec::new();
    let mut functional_iris: HashSet<String> = HashSet::new();
    let mut restriction_bnodes: Vec<String> = Vec::new();

    for t in triples {
        if t.predicate != RDF_TYPE {
            continue;
        }
        let Some(type_iri) = t.object.as_iri() else {
            continue;
        };
        match type_iri {
            OWL_ONTOLOGY => {
                // Extract ontology label/comment from sibling triples
                if let Some(siblings) = subject_index.get(t.subject.as_str()) {
                    for s in siblings {
                        if s.predicate == RDFS_LABEL {
                            ontology_name = s.object.as_literal_value().map(|v| v.to_string());
                        } else if s.predicate == RDFS_COMMENT {
                            ontology_description =
                                s.object.as_literal_value().map(|v| v.to_string());
                        }
                    }
                }
            }
            OWL_CLASS => class_iris.push(t.subject.clone()),
            OWL_DATATYPE_PROPERTY => datatype_prop_iris.push(t.subject.clone()),
            OWL_OBJECT_PROPERTY => object_prop_iris.push(t.subject.clone()),
            OWL_FUNCTIONAL_PROPERTY => {
                functional_iris.insert(t.subject.clone());
            }
            OWL_RESTRICTION => restriction_bnodes.push(t.subject.clone()),
            other => {
                // Skip unknown OWL constructs (e.g., owl:AnnotationProperty)
                if other.starts_with("http://www.w3.org/2002/07/owl#") {
                    warn!(
                        subject = %t.subject,
                        owl_type = %other,
                        "Skipping unsupported OWL construct"
                    );
                }
            }
        }
    }

    // Deduplicate (a property can be both DatatypeProperty and FunctionalProperty)
    class_iris.dedup();
    datatype_prop_iris.dedup();
    object_prop_iris.dedup();

    // --- Extract class info ---
    let mut classes: BTreeMap<String, ClassInfo> = BTreeMap::new();
    for iri in &class_iris {
        let mut info = ClassInfo {
            iri: iri.clone(),
            label: None,
            description: None,
        };
        if let Some(siblings) = subject_index.get(iri.as_str()) {
            for s in siblings {
                match s.predicate.as_str() {
                    RDFS_LABEL => info.label = s.object.as_literal_value().map(|v| v.to_string()),
                    RDFS_COMMENT => {
                        info.description = s.object.as_literal_value().map(|v| v.to_string())
                    }
                    _ => {}
                }
            }
        }
        classes.insert(iri.clone(), info);
    }

    // --- Extract datatype property info ---
    let mut datatype_props: Vec<DatatypePropInfo> = Vec::new();
    for iri in &datatype_prop_iris {
        let mut info = DatatypePropInfo {
            iri: iri.clone(),
            label: None,
            description: None,
            domain: None,
            range: None,
            is_functional: functional_iris.contains(iri),
        };
        if let Some(siblings) = subject_index.get(iri.as_str()) {
            for s in siblings {
                match s.predicate.as_str() {
                    RDFS_LABEL => info.label = s.object.as_literal_value().map(|v| v.to_string()),
                    RDFS_COMMENT => {
                        // Use first comment as property description, skip relationship annotations
                        if info.description.is_none() {
                            let val = s.object.as_literal_value().map(|v| v.to_string());
                            // Filter out "Property on relationship X" export annotations
                            if let Some(ref v) = val
                                && !v.starts_with("Property on relationship ")
                            {
                                info.description = Some(v.clone());
                            }
                        }
                    }
                    RDFS_DOMAIN => info.domain = s.object.as_iri().map(|v| v.to_string()),
                    RDFS_RANGE => info.range = s.object.as_iri().map(|v| v.to_string()),
                    _ => {}
                }
            }
        }
        datatype_props.push(info);
    }

    // --- Extract object property info ---
    let mut object_props: Vec<ObjectPropInfo> = Vec::new();
    for iri in &object_prop_iris {
        let mut info = ObjectPropInfo {
            iri: iri.clone(),
            label: None,
            description: None,
            domain: None,
            range: None,
        };
        if let Some(siblings) = subject_index.get(iri.as_str()) {
            for s in siblings {
                match s.predicate.as_str() {
                    RDFS_LABEL => info.label = s.object.as_literal_value().map(|v| v.to_string()),
                    RDFS_COMMENT => {
                        info.description = s.object.as_literal_value().map(|v| v.to_string())
                    }
                    RDFS_DOMAIN => info.domain = s.object.as_iri().map(|v| v.to_string()),
                    RDFS_RANGE => info.range = s.object.as_iri().map(|v| v.to_string()),
                    _ => {}
                }
            }
        }
        object_props.push(info);
    }

    // --- Extract restriction info ---
    let mut restrictions: Vec<(String, RestrictionInfo)> = Vec::new(); // (class_iri, restriction)
    for bnode_id in &restriction_bnodes {
        let mut rinfo = RestrictionInfo::default();
        if let Some(siblings) = subject_index.get(bnode_id.as_str()) {
            for s in siblings {
                match s.predicate.as_str() {
                    OWL_ON_PROPERTY => {
                        rinfo.on_property = s.object.as_iri().map(|v| v.to_string());
                        // Also handle blank node references to properties
                        if rinfo.on_property.is_none()
                            && let OwnedTerm::Blank(ref b) = s.object
                        {
                            rinfo.on_property = Some(b.clone());
                        }
                    }
                    OWL_MAX_CARDINALITY | OWL_MAX_QUALIFIED_CARDINALITY => {
                        rinfo.max_cardinality =
                            s.object.as_literal_value().and_then(|v| v.parse().ok());
                    }
                    OWL_MIN_CARDINALITY | OWL_MIN_QUALIFIED_CARDINALITY => {
                        rinfo.min_cardinality =
                            s.object.as_literal_value().and_then(|v| v.parse().ok());
                    }
                    _ => {}
                }
            }
        }

        // Find which class this restriction belongs to via rdfs:subClassOf
        let class_iri = triples
            .iter()
            .find(|t| {
                t.predicate == RDFS_SUB_CLASS_OF
                    && match &t.object {
                        OwnedTerm::Blank(b) => b == bnode_id,
                        OwnedTerm::Iri(i) => i == bnode_id,
                        _ => false,
                    }
            })
            .map(|t| t.subject.clone());

        if let Some(class_iri) = class_iri {
            restrictions.push((class_iri, rinfo));
        }
    }

    // --- Build restriction lookup: property_iri → (max_card, min_card) per class ---
    let mut restriction_map: HashMap<String, RestrictionInfo> = HashMap::new();
    for (_class_iri, rinfo) in &restrictions {
        if let Some(prop_iri) = &rinfo.on_property {
            let entry = restriction_map.entry(prop_iri.clone()).or_default();
            if rinfo.max_cardinality.is_some() {
                entry.max_cardinality = rinfo.max_cardinality;
            }
            if rinfo.min_cardinality.is_some() {
                entry.min_cardinality = rinfo.min_cardinality;
            }
        }
    }

    // --- Build class IRI → class label mapping ---
    let class_label_for = |iri: &str| -> String {
        classes
            .get(iri)
            .and_then(|c| c.label.clone())
            .unwrap_or_else(|| local_name_from_iri(iri))
    };

    // Set of known class IRIs for validation
    let class_iri_set: HashSet<&str> = classes.keys().map(|s| s.as_str()).collect();

    // --- Detect edge properties (datatype properties whose domain comment indicates
    //     "Property on relationship X" — exported by generate_owl_turtle) ---
    //     These are datatype properties that use the pattern: :{EdgeLabel}_{PropName}
    //     with a rdfs:comment "Property on relationship {EdgeLabel}"
    let edge_prop_iris: HashSet<String> = datatype_props
        .iter()
        .filter(|dp| {
            if let Some(siblings) = subject_index.get(dp.iri.as_str()) {
                siblings.iter().any(|s| {
                    s.predicate == RDFS_COMMENT
                        && s.object
                            .as_literal_value()
                            .is_some_and(|v| v.starts_with("Property on relationship "))
                })
            } else {
                false
            }
        })
        .map(|dp| dp.iri.clone())
        .collect();

    // --- Build edge property lookup: edge_label → Vec<InputPropertyDef> ---
    let mut edge_property_map: HashMap<String, Vec<InputPropertyDef>> = HashMap::new();
    for dp in &datatype_props {
        if !edge_prop_iris.contains(&dp.iri) {
            continue;
        }
        // Extract the edge label from the comment
        if let Some(siblings) = subject_index.get(dp.iri.as_str()) {
            for s in siblings {
                if s.predicate == RDFS_COMMENT
                    && let Some(comment) = s.object.as_literal_value()
                    && let Some(edge_label) = comment.strip_prefix("Property on relationship ")
                {
                    let prop = InputPropertyDef {
                        id: None,
                        name: dp
                            .label
                            .clone()
                            .unwrap_or_else(|| local_name_from_iri(&dp.iri)),
                        property_type: dp
                            .range
                            .as_deref()
                            .map(xsd_to_property_type)
                            .unwrap_or(PropertyType::String),
                        nullable: true,
                        default_value: None,
                        description: dp.description.clone(),
                        source_column: None,
                    };
                    edge_property_map
                        .entry(edge_label.to_string())
                        .or_default()
                        .push(prop);
                }
            }
        }
    }

    // --- Build node types ---
    // Group datatype properties by domain class
    let mut class_properties: BTreeMap<String, Vec<(DatatypePropInfo, bool)>> = BTreeMap::new();
    for dp in &datatype_props {
        // Skip edge properties
        if edge_prop_iris.contains(&dp.iri) {
            continue;
        }

        if let Some(domain) = &dp.domain {
            if class_iri_set.contains(domain.as_str()) {
                class_properties.entry(domain.clone()).or_default().push((
                    DatatypePropInfo {
                        iri: dp.iri.clone(),
                        label: dp.label.clone(),
                        description: dp.description.clone(),
                        domain: dp.domain.clone(),
                        range: dp.range.clone(),
                        is_functional: dp.is_functional,
                    },
                    dp.is_functional,
                ));
            } else {
                warn!(
                    property = %dp.iri,
                    domain = %domain,
                    "Datatype property domain is not a known class, skipping"
                );
            }
        } else {
            warn!(
                property = %dp.iri,
                "Datatype property has no domain, skipping"
            );
        }
    }

    let mut node_types: Vec<InputNodeTypeDef> = Vec::new();
    for (class_iri, class_info) in &classes {
        let label = class_info
            .label
            .clone()
            .unwrap_or_else(|| local_name_from_iri(class_iri));

        let mut properties = Vec::new();
        let mut constraints = Vec::new();

        if let Some(props) = class_properties.get(class_iri) {
            for (dp, is_functional) in props {
                let prop_name = dp
                    .label
                    .clone()
                    .unwrap_or_else(|| local_name_from_iri(&dp.iri));

                properties.push(InputPropertyDef {
                    id: None,
                    name: prop_name.clone(),
                    property_type: dp
                        .range
                        .as_deref()
                        .map(xsd_to_property_type)
                        .unwrap_or(PropertyType::String),
                    nullable: true,
                    default_value: None,
                    description: dp.description.clone(),
                    source_column: None,
                });

                if *is_functional {
                    constraints.push(InputNodeConstraint::Unique {
                        id: None,
                        properties: vec![prop_name],
                    });
                }
            }
        }

        node_types.push(InputNodeTypeDef {
            id: None,
            label,
            description: class_info.description.clone(),
            source_table: None,
            properties,
            constraints,
        });
    }

    // --- Build edge types ---
    let mut edge_types: Vec<InputEdgeTypeDef> = Vec::new();
    for op in &object_props {
        let label = op
            .label
            .clone()
            .unwrap_or_else(|| local_name_from_iri(&op.iri));

        let source_label = match &op.domain {
            Some(domain) if class_iri_set.contains(domain.as_str()) => class_label_for(domain),
            Some(domain) => {
                warn!(
                    property = %op.iri,
                    domain = %domain,
                    "Object property domain is not a known class, skipping"
                );
                continue;
            }
            None => {
                warn!(
                    property = %op.iri,
                    "Object property has no domain, skipping"
                );
                continue;
            }
        };

        let target_label = match &op.range {
            Some(range) if class_iri_set.contains(range.as_str()) => class_label_for(range),
            Some(range) => {
                warn!(
                    property = %op.iri,
                    range = %range,
                    "Object property range is not a known class, skipping"
                );
                continue;
            }
            None => {
                warn!(
                    property = %op.iri,
                    "Object property has no range, skipping"
                );
                continue;
            }
        };

        // Determine cardinality from restrictions
        let cardinality = match restriction_map.get(&op.iri) {
            Some(rinfo) => match (rinfo.max_cardinality, rinfo.min_cardinality) {
                (Some(1), _) => Cardinality::ManyToOne,
                _ => Cardinality::ManyToMany,
            },
            None => Cardinality::ManyToMany,
        };

        // Collect edge properties if any
        let edge_props = edge_property_map.remove(&label).unwrap_or_default();

        edge_types.push(InputEdgeTypeDef {
            id: None,
            label,
            description: op.description.clone(),
            source_type: source_label,
            target_type: target_label,
            properties: edge_props,
            cardinality,
        });
    }

    // Determine ontology name
    let name = ontology_name.unwrap_or_else(|| "Imported Ontology".to_string());

    if node_types.is_empty() {
        return Err(OxError::Compilation {
            message: "No owl:Class declarations found in Turtle input".to_string(),
        });
    }

    Ok(OntologyInputIR {
        format_version: 1,
        id: None,
        name,
        description: ontology_description,
        version: 1,
        node_types,
        edge_types,
        indexes: vec![],
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a local name from an IRI (fragment or last path segment).
fn local_name_from_iri(iri: &str) -> String {
    if let Some(pos) = iri.rfind('#') {
        iri[pos + 1..].to_string()
    } else if let Some(pos) = iri.rfind('/') {
        iri[pos + 1..].to_string()
    } else {
        iri.to_string()
    }
}

/// Map an XSD datatype IRI to a PropertyType.
fn xsd_to_property_type(xsd_iri: &str) -> PropertyType {
    match xsd_iri {
        XSD_STRING => PropertyType::String,
        XSD_INTEGER
        | XSD_INT
        | XSD_LONG
        | XSD_SHORT
        | XSD_BYTE
        | XSD_NON_NEGATIVE_INTEGER
        | XSD_POSITIVE_INTEGER
        | XSD_UNSIGNED_INT
        | XSD_UNSIGNED_LONG => PropertyType::Int,
        XSD_DOUBLE | XSD_FLOAT | XSD_DECIMAL => PropertyType::Float,
        XSD_BOOLEAN => PropertyType::Bool,
        XSD_DATE => PropertyType::Date,
        XSD_DATE_TIME => PropertyType::DateTime,
        XSD_DURATION => PropertyType::Duration,
        XSD_BASE64_BINARY | XSD_HEX_BINARY => PropertyType::Bytes,
        other => {
            warn!(datatype = %other, "Unknown XSD datatype, defaulting to String");
            PropertyType::String
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, OntologyIR,
        PropertyDef,
    };
    use ox_core::types::PropertyType;

    /// Helper: generate OWL Turtle from an OntologyIR using the export module,
    /// then parse it back.
    fn roundtrip(ontology: &OntologyIR) -> OntologyIR {
        let turtle = crate::export::generate_owl_turtle(ontology);
        parse_owl_turtle(&turtle).expect("roundtrip parse should succeed")
    }

    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "test-id".into(),
            "Cosmetics".into(),
            Some("Korean cosmetics ontology".into()),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Brand".into(),
                    description: Some("Cosmetic brand entity".into()),
                    source_table: None,
                    properties: vec![
                        PropertyDef {
                            id: "p1".into(),
                            name: "name".into(),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: Some("Brand name in Korean".into()),
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: "founded_year".into(),
                            property_type: PropertyType::Int,
                            nullable: true,
                            default_value: None,
                            description: None,
                        },
                    ],
                    constraints: vec![ConstraintDef {
                        id: "c1".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["p1".into()],
                        },
                    }],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Product".into(),
                    description: Some("A cosmetic product".into()),
                    source_table: None,
                    properties: vec![PropertyDef {
                        id: "p3".into(),
                        name: "price".into(),
                        property_type: PropertyType::Float,
                        nullable: false,
                        default_value: None,
                        description: None,
                    }],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "MANUFACTURED_BY".into(),
                description: Some("Product manufactured by brand".into()),
                source_node_id: "n2".into(),
                target_node_id: "n1".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
            }],
            vec![],
        )
    }

    #[test]
    fn test_parse_simple_class() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ;
                rdfs:label "Test" .

            :Person a owl:Class ;
                rdfs:label "Person" ;
                rdfs:comment "A human being" .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        assert_eq!(result.name, "Test");
        assert_eq!(result.node_types.len(), 1);
        assert_eq!(result.node_types[0].label, "Person");
        assert_eq!(
            result.node_types[0].description,
            Some("A human being".to_string())
        );
    }

    #[test]
    fn test_parse_datatype_property() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :Person a owl:Class ; rdfs:label "Person" .

            :Person_name a owl:DatatypeProperty ;
                rdfs:label "name" ;
                rdfs:domain :Person ;
                rdfs:range xsd:string .

            :Person_age a owl:DatatypeProperty ;
                rdfs:label "age" ;
                rdfs:domain :Person ;
                rdfs:range xsd:integer .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        let person = &result.node_types[0];
        assert_eq!(person.properties.len(), 2);

        let name_prop = person.properties.iter().find(|p| p.name == "name").unwrap();
        assert_eq!(name_prop.property_type, PropertyType::String);

        let age_prop = person.properties.iter().find(|p| p.name == "age").unwrap();
        assert_eq!(age_prop.property_type, PropertyType::Int);
    }

    #[test]
    fn test_parse_object_property() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :Person a owl:Class ; rdfs:label "Person" .
            :Company a owl:Class ; rdfs:label "Company" .

            :WORKS_FOR a owl:ObjectProperty ;
                rdfs:label "WORKS_FOR" ;
                rdfs:comment "Employment relationship" ;
                rdfs:domain :Person ;
                rdfs:range :Company .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        assert_eq!(result.edge_types.len(), 1);

        let edge = &result.edge_types[0];
        assert_eq!(edge.label, "WORKS_FOR");
        assert_eq!(
            edge.description,
            Some("Employment relationship".to_string())
        );

        // Source should be Person, target should be Company
        let source = result.node_label(&edge.source_node_id).unwrap();
        let target = result.node_label(&edge.target_node_id).unwrap();
        assert_eq!(source, "Person");
        assert_eq!(target, "Company");
    }

    #[test]
    fn test_parse_functional_property() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :Person a owl:Class ; rdfs:label "Person" .

            :Person_email a owl:DatatypeProperty , owl:FunctionalProperty ;
                rdfs:label "email" ;
                rdfs:domain :Person ;
                rdfs:range xsd:string .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        let person = &result.node_types[0];

        // Should have a Unique constraint on "email"
        assert!(person.has_unique_constraint());
        let unique = person
            .constraints
            .iter()
            .find(|c| matches!(&c.constraint, NodeConstraint::Unique { .. }))
            .unwrap();
        match &unique.constraint {
            NodeConstraint::Unique { property_ids } => {
                assert_eq!(property_ids.len(), 1);
                // The property_id should reference the email property
                let email_prop = person
                    .properties
                    .iter()
                    .find(|p| p.name == "email")
                    .unwrap();
                assert_eq!(property_ids[0], email_prop.id);
            }
            _ => panic!("expected Unique constraint"),
        }
    }

    #[test]
    fn test_parse_cardinality_restriction() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :Employee a owl:Class ; rdfs:label "Employee" .
            :Department a owl:Class ; rdfs:label "Department" .

            :BELONGS_TO a owl:ObjectProperty ;
                rdfs:label "BELONGS_TO" ;
                rdfs:domain :Employee ;
                rdfs:range :Department .

            :Employee rdfs:subClassOf [
                a owl:Restriction ;
                owl:onProperty :BELONGS_TO ;
                owl:maxCardinality "1"^^xsd:nonNegativeInteger
            ] .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        let edge = &result.edge_types[0];
        assert_eq!(edge.cardinality, Cardinality::ManyToOne);
    }

    #[test]
    fn test_parse_multiple_classes() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ;
                rdfs:label "E-Commerce" ;
                rdfs:comment "Online store ontology" .

            :Customer a owl:Class ; rdfs:label "Customer" .
            :Product a owl:Class ; rdfs:label "Product" .
            :Order a owl:Class ; rdfs:label "Order" .

            :Customer_email a owl:DatatypeProperty ;
                rdfs:label "email" ;
                rdfs:domain :Customer ;
                rdfs:range xsd:string .

            :Product_price a owl:DatatypeProperty ;
                rdfs:label "price" ;
                rdfs:domain :Product ;
                rdfs:range xsd:decimal .

            :Order_date a owl:DatatypeProperty ;
                rdfs:label "date" ;
                rdfs:domain :Order ;
                rdfs:range xsd:dateTime .

            :PLACED a owl:ObjectProperty ;
                rdfs:label "PLACED" ;
                rdfs:domain :Customer ;
                rdfs:range :Order .

            :CONTAINS a owl:ObjectProperty ;
                rdfs:label "CONTAINS" ;
                rdfs:domain :Order ;
                rdfs:range :Product .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();

        assert_eq!(result.name, "E-Commerce");
        assert_eq!(
            result.description,
            Some("Online store ontology".to_string())
        );
        assert_eq!(result.node_types.len(), 3);
        assert_eq!(result.edge_types.len(), 2);

        // Verify each class has its property
        let customer = result
            .node_types
            .iter()
            .find(|n| n.label == "Customer")
            .unwrap();
        assert!(customer.properties.iter().any(|p| p.name == "email"));

        let product = result
            .node_types
            .iter()
            .find(|n| n.label == "Product")
            .unwrap();
        assert!(
            product
                .properties
                .iter()
                .any(|p| p.name == "price" && p.property_type == PropertyType::Float)
        );

        let order = result
            .node_types
            .iter()
            .find(|n| n.label == "Order")
            .unwrap();
        assert!(
            order
                .properties
                .iter()
                .any(|p| p.name == "date" && p.property_type == PropertyType::DateTime)
        );
    }

    #[test]
    fn test_xsd_type_mapping() {
        assert_eq!(xsd_to_property_type(XSD_STRING), PropertyType::String);
        assert_eq!(xsd_to_property_type(XSD_INTEGER), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_INT), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_LONG), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_SHORT), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_BYTE), PropertyType::Int);
        assert_eq!(
            xsd_to_property_type(XSD_NON_NEGATIVE_INTEGER),
            PropertyType::Int
        );
        assert_eq!(
            xsd_to_property_type(XSD_POSITIVE_INTEGER),
            PropertyType::Int
        );
        assert_eq!(xsd_to_property_type(XSD_UNSIGNED_INT), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_UNSIGNED_LONG), PropertyType::Int);
        assert_eq!(xsd_to_property_type(XSD_DOUBLE), PropertyType::Float);
        assert_eq!(xsd_to_property_type(XSD_FLOAT), PropertyType::Float);
        assert_eq!(xsd_to_property_type(XSD_DECIMAL), PropertyType::Float);
        assert_eq!(xsd_to_property_type(XSD_BOOLEAN), PropertyType::Bool);
        assert_eq!(xsd_to_property_type(XSD_DATE), PropertyType::Date);
        assert_eq!(xsd_to_property_type(XSD_DATE_TIME), PropertyType::DateTime);
        assert_eq!(xsd_to_property_type(XSD_DURATION), PropertyType::Duration);
        assert_eq!(xsd_to_property_type(XSD_BASE64_BINARY), PropertyType::Bytes);
        assert_eq!(xsd_to_property_type(XSD_HEX_BINARY), PropertyType::Bytes);
        // Unknown falls back to String
        assert_eq!(
            xsd_to_property_type("http://example.org/custom"),
            PropertyType::String
        );
    }

    #[test]
    fn test_roundtrip_export_import() {
        let original = sample_ontology();
        let imported = roundtrip(&original);

        // Structure should be equivalent
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.description, original.description);
        assert_eq!(imported.node_types.len(), original.node_types.len());
        assert_eq!(imported.edge_types.len(), original.edge_types.len());

        // Check each node type
        for orig_node in &original.node_types {
            let imp_node = imported
                .node_types
                .iter()
                .find(|n| n.label == orig_node.label)
                .unwrap_or_else(|| panic!("Missing node type: {}", orig_node.label));

            assert_eq!(imp_node.description, orig_node.description);
            assert_eq!(imp_node.properties.len(), orig_node.properties.len());

            for orig_prop in &orig_node.properties {
                let imp_prop = imp_node
                    .properties
                    .iter()
                    .find(|p| p.name == orig_prop.name)
                    .unwrap_or_else(|| {
                        panic!("Missing property: {}.{}", orig_node.label, orig_prop.name)
                    });
                assert_eq!(imp_prop.property_type, orig_prop.property_type);
            }
        }

        // Check edge types
        for orig_edge in &original.edge_types {
            let imp_edge = imported
                .edge_types
                .iter()
                .find(|e| e.label == orig_edge.label)
                .unwrap_or_else(|| panic!("Missing edge type: {}", orig_edge.label));

            assert_eq!(imp_edge.description, orig_edge.description);
            assert_eq!(imp_edge.cardinality, orig_edge.cardinality);

            // Verify source/target by label
            let orig_source = original.node_label(&orig_edge.source_node_id).unwrap();
            let imp_source = imported.node_label(&imp_edge.source_node_id).unwrap();
            assert_eq!(orig_source, imp_source);

            let orig_target = original.node_label(&orig_edge.target_node_id).unwrap();
            let imp_target = imported.node_label(&imp_edge.target_node_id).unwrap();
            assert_eq!(orig_target, imp_target);
        }

        // Check unique constraint survived roundtrip
        let brand = imported
            .node_types
            .iter()
            .find(|n| n.label == "Brand")
            .unwrap();
        assert!(brand.has_unique_constraint());
    }

    #[test]
    fn test_missing_domain_range() {
        // Properties without domain should be skipped gracefully
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :Thing a owl:Class ; rdfs:label "Thing" .

            :orphanProp a owl:DatatypeProperty ;
                rdfs:label "orphan" ;
                rdfs:range xsd:string .

            :orphanEdge a owl:ObjectProperty ;
                rdfs:label "orphanEdge" ;
                rdfs:domain :Thing .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        // orphanProp should be skipped (no domain)
        let thing = &result.node_types[0];
        assert!(thing.properties.is_empty());
        // orphanEdge should be skipped (no range)
        assert!(result.edge_types.is_empty());
    }

    #[test]
    fn test_invalid_turtle() {
        let bad = "this is not valid turtle @@@!!";
        let err = parse_owl_turtle(bad).unwrap_err();
        match err {
            OxError::Compilation { message } => {
                assert!(message.contains("Failed to parse Turtle"));
            }
            _ => panic!("expected Compilation error"),
        }
    }

    #[test]
    fn test_no_classes_error() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Empty" .
        "#;

        let err = parse_owl_turtle(turtle).unwrap_err();
        match err {
            OxError::Compilation { message } => {
                assert!(message.contains("No owl:Class declarations"));
            }
            _ => panic!("expected Compilation error"),
        }
    }

    #[test]
    fn test_class_without_label_uses_local_name() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .
            :MyClass a owl:Class .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        assert_eq!(result.node_types[0].label, "MyClass");
    }

    #[test]
    fn test_many_to_many_default_cardinality() {
        let turtle = r#"
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix :     <http://example.org#> .

            <http://example.org> a owl:Ontology ; rdfs:label "Test" .

            :A a owl:Class ; rdfs:label "A" .
            :B a owl:Class ; rdfs:label "B" .

            :LINKS a owl:ObjectProperty ;
                rdfs:label "LINKS" ;
                rdfs:domain :A ;
                rdfs:range :B .
        "#;

        let result = parse_owl_turtle(turtle).unwrap();
        assert_eq!(result.edge_types[0].cardinality, Cardinality::ManyToMany);
    }

    #[test]
    fn test_edge_properties_roundtrip() {
        // Create ontology with edge properties
        let ontology = OntologyIR::new(
            "test".into(),
            "Test".into(),
            None,
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Person".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Company".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "WORKS_AT".into(),
                description: None,
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![PropertyDef {
                    id: "ep1".into(),
                    name: "since".into(),
                    property_type: PropertyType::Date,
                    nullable: true,
                    default_value: None,
                    description: None,
                }],
                cardinality: Cardinality::ManyToMany,
            }],
            vec![],
        );

        let imported = roundtrip(&ontology);

        // Edge properties should survive roundtrip
        let edge = imported
            .edge_types
            .iter()
            .find(|e| e.label == "WORKS_AT")
            .unwrap();
        assert_eq!(edge.properties.len(), 1);
        assert_eq!(edge.properties[0].name, "since");
        assert_eq!(edge.properties[0].property_type, PropertyType::Date);
    }
}
