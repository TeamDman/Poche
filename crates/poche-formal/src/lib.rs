//! A finite, pure expression dialect carried by Weavy.
//!
//! The types in this crate deliberately describe less than Rust can execute.
//! Every accepted expression is finite, total for well-typed inputs, inspectable,
//! and executable without calling back into an opaque host function.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use weavy::{Control, Lowered, RunError, Step};

/// A closed integer interval used as a finite integer type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntRange {
    min: i32,
    max: i32,
}

impl IntRange {
    /// Construct a non-empty integer interval.
    ///
    /// # Errors
    ///
    /// Returns an error when `min` is greater than `max`.
    pub fn new(min: i32, max: i32) -> Result<Self, SchemaError> {
        if min > max {
            return Err(SchemaError::new(format!(
                "integer range {min}..={max} is empty"
            )));
        }
        Ok(Self { min, max })
    }

    /// Return the inclusive lower bound.
    #[must_use]
    pub const fn min(self) -> i32 {
        self.min
    }

    /// Return the inclusive upper bound.
    #[must_use]
    pub const fn max(self) -> i32 {
        self.max
    }

    const fn contains(self, value: i32) -> bool {
        self.min <= value && value <= self.max
    }
}

/// A named finite enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumType {
    name: String,
    variants: Vec<String>,
}

impl EnumType {
    /// Construct an enum with at least one uniquely named variant.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty name, no variants, empty variants, or
    /// duplicate variants.
    pub fn new(
        name: impl Into<String>,
        variants: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, SchemaError> {
        let name = name.into();
        let variants: Vec<String> = variants.into_iter().map(Into::into).collect();
        ensure_name("enum", &name)?;
        if variants.is_empty() {
            return Err(SchemaError::new(format!(
                "enum {name:?} must have at least one variant"
            )));
        }
        let mut seen = BTreeSet::new();
        for variant in &variants {
            ensure_name("enum variant", variant)?;
            if !seen.insert(variant.clone()) {
                return Err(SchemaError::new(format!(
                    "enum {name:?} repeats variant {variant:?}"
                )));
            }
        }
        Ok(Self { name, variants })
    }

    /// Return the stable enum name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the variants in ordinal order.
    #[must_use]
    pub fn variants(&self) -> &[String] {
        &self.variants
    }

    /// Resolve a variant name to its ordinal.
    #[must_use]
    pub fn ordinal(&self, variant: &str) -> Option<usize> {
        self.variants
            .iter()
            .position(|candidate| candidate == variant)
    }
}

/// One named field in a finite record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldType {
    name: String,
    ty: Type,
}

impl FieldType {
    /// Construct a named field.
    ///
    /// # Errors
    ///
    /// Returns an error when the field name is empty.
    pub fn new(name: impl Into<String>, ty: Type) -> Result<Self, SchemaError> {
        let name = name.into();
        ensure_name("record field", &name)?;
        Ok(Self { name, ty })
    }

    /// Return the field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the field type.
    #[must_use]
    pub const fn ty(&self) -> &Type {
        &self.ty
    }
}

/// A named finite product type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordType {
    name: String,
    fields: Vec<FieldType>,
}

impl RecordType {
    /// Construct a record with uniquely named fields.
    ///
    /// Empty records are valid finite products.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty record name or duplicate field names.
    pub fn new(
        name: impl Into<String>,
        fields: impl IntoIterator<Item = FieldType>,
    ) -> Result<Self, SchemaError> {
        let name = name.into();
        ensure_name("record", &name)?;
        let fields: Vec<FieldType> = fields.into_iter().collect();
        let mut seen = BTreeSet::new();
        for field in &fields {
            if !seen.insert(field.name.clone()) {
                return Err(SchemaError::new(format!(
                    "record {name:?} repeats field {:?}",
                    field.name
                )));
            }
        }
        Ok(Self { name, fields })
    }

    /// Return the stable record name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the ordered record fields.
    #[must_use]
    pub fn fields(&self) -> &[FieldType] {
        &self.fields
    }

    fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|field| field.name == name)
    }
}

/// A type admitted by the formal dialect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// A Boolean value.
    Bool,
    /// A finite integer interval.
    Integer(IntRange),
    /// A named finite enumeration.
    Enumeration(EnumType),
    /// A named product with statically known fields.
    Record(RecordType),
    /// A homogeneous array with statically known length.
    FixedArray {
        /// Element type.
        element: Box<Type>,
        /// Exact element count.
        len: usize,
    },
}

impl Type {
    /// Construct a fixed-size array type.
    #[must_use]
    pub fn fixed_array(element: Self, len: usize) -> Self {
        Self::FixedArray {
            element: Box::new(element),
            len,
        }
    }

    /// Test whether a runtime value belongs to this finite type.
    #[must_use]
    pub fn accepts(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::Bool, Value::Bool(_)) => true,
            (Self::Integer(range), Value::Integer(value)) => range.contains(*value),
            (Self::Enumeration(schema), Value::Enumeration { type_name, ordinal }) => {
                type_name == schema.name() && *ordinal < schema.variants().len()
            }
            (Self::Record(schema), Value::Record { type_name, fields }) => {
                type_name == schema.name()
                    && fields.len() == schema.fields().len()
                    && fields
                        .iter()
                        .zip(schema.fields())
                        .all(|(value, field)| field.ty.accepts(value))
            }
            (Self::FixedArray { element, len }, Value::FixedArray(values)) => {
                values.len() == *len && values.iter().all(|value| element.accepts(value))
            }
            _ => false,
        }
    }
}

/// A concrete value admitted by the formal dialect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A Boolean value.
    Bool(bool),
    /// An integer value whose bounds are supplied by its expression type.
    Integer(i32),
    /// A named enumeration value.
    Enumeration {
        /// Stable enum type name.
        type_name: String,
        /// Zero-based variant ordinal.
        ordinal: usize,
    },
    /// A named record value with fields in schema order.
    Record {
        /// Stable record type name.
        type_name: String,
        /// Field values in schema order.
        fields: Vec<Value>,
    },
    /// An exactly sized array value.
    FixedArray(Vec<Value>),
}

/// The semantic role of a formal computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComputationKind {
    /// State construction or refinement.
    State,
    /// Per-agent observation construction.
    Observation,
    /// Legal player or chance action calculation.
    LegalAction,
    /// State transition calculation.
    Transition,
    /// Round or game scoring calculation.
    Scoring,
    /// A safety, consistency, or liveness predicate.
    Property,
}

/// A stable rulebook location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRef {
    document: String,
    line: u32,
    clause: String,
}

impl SourceRef {
    /// Construct a source location.
    #[must_use]
    pub fn new(document: impl Into<String>, line: u32, clause: impl Into<String>) -> Self {
        Self {
            document: document.into(),
            line,
            clause: clause.into(),
        }
    }

    /// Return the source document.
    #[must_use]
    pub fn document(&self) -> &str {
        &self.document
    }

    /// Return the one-based source line, or zero when not line-addressable.
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Return the human-readable clause label.
    #[must_use]
    pub fn clause(&self) -> &str {
        &self.clause
    }
}

/// Traceability attached to every expression and lowered instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    rule_id: String,
    source: SourceRef,
    computation: ComputationKind,
}

impl Origin {
    /// Construct rule and source provenance.
    #[must_use]
    pub fn new(
        rule_id: impl Into<String>,
        source: SourceRef,
        computation: ComputationKind,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            source,
            computation,
        }
    }

    /// Return the stable rule identifier.
    #[must_use]
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// Return the rulebook source location.
    #[must_use]
    pub const fn source(&self) -> &SourceRef {
        &self.source
    }

    /// Return the computation role.
    #[must_use]
    pub const fn computation(&self) -> ComputationKind {
        self.computation
    }
}

/// A schema-construction error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError {
    message: String,
}

impl SchemaError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SchemaError {}

/// A source-oriented graph construction, lowering, or evaluation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    message: String,
    origin: Box<Origin>,
    instruction: Option<usize>,
}

impl Diagnostic {
    fn at_origin(message: impl Into<String>, origin: &Origin) -> Self {
        Self {
            message: message.into(),
            origin: Box::new(origin.clone()),
            instruction: None,
        }
    }

    fn at_instruction(message: impl Into<String>, origin: &Origin, instruction: usize) -> Self {
        Self {
            message: message.into(),
            origin: Box::new(origin.clone()),
            instruction: Some(instruction),
        }
    }

    /// Return the diagnostic text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Return the responsible rule/source origin.
    #[must_use]
    pub const fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Return the lowered instruction offset for evaluation failures.
    #[must_use]
    pub const fn instruction(&self) -> Option<usize> {
        self.instruction
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} [{} at {}:{} ({})]",
            self.message,
            self.origin.rule_id,
            self.origin.source.document,
            self.origin.source.line,
            self.origin.source.clause
        )?;
        if let Some(instruction) = self.instruction {
            write!(formatter, " [instruction {instruction}]")?;
        }
        Ok(())
    }
}

impl Error for Diagnostic {}

/// Rust capabilities that cannot enter a portable formal computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedFeature {
    /// Calling behavior that is not represented in the graph.
    OpaqueHostCall,
    /// I/O, mutation outside the formal state, clocks, or other effects.
    Effect,
    /// Runtime-sized allocation.
    DynamicAllocation,
    /// Iteration without a static finite bound.
    UnboundedIteration,
    /// Randomness that is not represented by an explicit chance input.
    HiddenRandomness,
    /// Arithmetic outside the bounded operations in this dialect.
    UnsupportedArithmetic,
}

impl UnsupportedFeature {
    const fn description(self) -> &'static str {
        match self {
            Self::OpaqueHostCall => "opaque host call",
            Self::Effect => "effectful operation",
            Self::DynamicAllocation => "dynamic allocation",
            Self::UnboundedIteration => "unbounded iteration",
            Self::HiddenRandomness => "hidden randomness",
            Self::UnsupportedArithmetic => "unsupported arithmetic",
        }
    }
}

/// A stable handle into an expression arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    /// Return the arena offset for diagnostics and serialization adapters.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
    ty: Type,
    origin: Origin,
}

#[derive(Clone, Debug)]
enum NodeKind {
    Constant(Value),
    Input(String),
    Enumeration {
        schema: EnumType,
        ordinal: usize,
    },
    EnumerationOrdinal {
        value: NodeId,
        variant_count: usize,
    },
    Record {
        schema: RecordType,
        fields: Vec<NodeId>,
    },
    Field {
        record: NodeId,
        index: usize,
    },
    Array {
        element_type: Type,
        elements: Vec<NodeId>,
    },
    Index {
        array: NodeId,
        index: NodeId,
    },
    Equal(NodeId, NodeId),
    Less(NodeId, NodeId),
    LessEqual(NodeId, NodeId),
    Not(NodeId),
    All(Vec<NodeId>),
    Any(Vec<NodeId>),
    Select {
        condition: NodeId,
        when_true: NodeId,
        when_false: NodeId,
    },
    Sum {
        values: Vec<NodeId>,
        range: IntRange,
    },
}

impl NodeKind {
    fn children(&self) -> Vec<NodeId> {
        match self {
            Self::Constant(_) | Self::Input(_) | Self::Enumeration { .. } => Vec::new(),
            Self::EnumerationOrdinal { value, .. }
            | Self::Not(value)
            | Self::Field { record: value, .. } => vec![*value],
            Self::Record { fields, .. }
            | Self::Array {
                elements: fields, ..
            }
            | Self::All(fields)
            | Self::Any(fields)
            | Self::Sum { values: fields, .. } => fields.clone(),
            Self::Index { array, index }
            | Self::Equal(array, index)
            | Self::Less(array, index)
            | Self::LessEqual(array, index) => vec![*array, *index],
            Self::Select {
                condition,
                when_true,
                when_false,
            } => vec![*condition, *when_true, *when_false],
        }
    }
}

/// Builder for the deliberately restricted formal expression graph.
#[derive(Clone, Debug, Default)]
pub struct GraphBuilder {
    nodes: Vec<Node>,
    inputs: BTreeMap<String, Type>,
}

impl GraphBuilder {
    /// Create an empty graph builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            inputs: BTreeMap::new(),
        }
    }

    /// Add a typed constant.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is outside `ty`.
    pub fn constant(
        &mut self,
        ty: Type,
        value: Value,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        if !ty.accepts(&value) {
            return Err(Diagnostic::at_origin(
                "constant does not belong to its declared finite type",
                &origin,
            ));
        }
        Ok(self.push(NodeKind::Constant(value), ty, origin))
    }

    /// Add a typed external input.
    ///
    /// Repeating a name with the same type is allowed and refers to the same
    /// runtime input. Repeating it with another type is rejected.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty name or a conflicting input declaration.
    pub fn input(
        &mut self,
        name: impl Into<String>,
        ty: Type,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Diagnostic::at_origin("input name is empty", &origin));
        }
        if let Some(existing) = self.inputs.get(&name) {
            if existing != &ty {
                return Err(Diagnostic::at_origin(
                    format!("input {name:?} was already declared with another type"),
                    &origin,
                ));
            }
        } else {
            self.inputs.insert(name.clone(), ty.clone());
        }
        Ok(self.push(NodeKind::Input(name), ty, origin))
    }

    /// Construct a named enumeration value.
    ///
    /// # Errors
    ///
    /// Returns an error when the variant is absent from the enum schema.
    pub fn enumeration(
        &mut self,
        schema: EnumType,
        variant: &str,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        let Some(ordinal) = schema.ordinal(variant) else {
            return Err(Diagnostic::at_origin(
                format!("enum {:?} has no variant {variant:?}", schema.name()),
                &origin,
            ));
        };
        let ty = Type::Enumeration(schema.clone());
        Ok(self.push(NodeKind::Enumeration { schema, ordinal }, ty, origin))
    }

    /// Obtain the bounded integer ordinal of an enumeration expression.
    ///
    /// # Errors
    ///
    /// Returns an error when the operand is not an enumeration.
    pub fn enumeration_ordinal(
        &mut self,
        value: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        let Type::Enumeration(schema) = self.node_type(value, &origin, "enum ordinal")? else {
            return Err(Diagnostic::at_origin(
                "enum ordinal requires an enumeration operand",
                &origin,
            ));
        };
        let variant_count = schema.variants().len();
        let max = usize_to_i32(variant_count - 1, &origin, "enum ordinal")?;
        Ok(self.push(
            NodeKind::EnumerationOrdinal {
                value,
                variant_count,
            },
            Type::Integer(IntRange { min: 0, max }),
            origin,
        ))
    }

    /// Construct a record from fields in schema order.
    ///
    /// # Errors
    ///
    /// Returns an error for an incorrect field count, unknown node, or field
    /// type mismatch.
    pub fn record(
        &mut self,
        schema: RecordType,
        fields: Vec<NodeId>,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        if fields.len() != schema.fields().len() {
            return Err(Diagnostic::at_origin(
                format!(
                    "record {:?} expects {} fields, received {}",
                    schema.name(),
                    schema.fields().len(),
                    fields.len()
                ),
                &origin,
            ));
        }
        for (node, field) in fields.iter().zip(schema.fields()) {
            if self.node_type(*node, &origin, "record construction")? != &field.ty {
                return Err(Diagnostic::at_origin(
                    format!("record field {:?} has the wrong type", field.name),
                    &origin,
                ));
            }
        }
        let ty = Type::Record(schema.clone());
        Ok(self.push(NodeKind::Record { schema, fields }, ty, origin))
    }

    /// Project a named field from a record expression.
    ///
    /// # Errors
    ///
    /// Returns an error when the operand is not a record or the field is absent.
    pub fn field(
        &mut self,
        record: NodeId,
        field: &str,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        let Type::Record(schema) = self.node_type(record, &origin, "record field")? else {
            return Err(Diagnostic::at_origin(
                "field projection requires a record operand",
                &origin,
            ));
        };
        let Some(index) = schema.field_index(field) else {
            return Err(Diagnostic::at_origin(
                format!("record {:?} has no field {field:?}", schema.name()),
                &origin,
            ));
        };
        let ty = schema.fields[index].ty.clone();
        Ok(self.push(NodeKind::Field { record, index }, ty, origin))
    }

    /// Construct an exactly sized homogeneous array.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown node or element type mismatch.
    pub fn fixed_array(
        &mut self,
        element_type: Type,
        elements: Vec<NodeId>,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        for element in &elements {
            if self.node_type(*element, &origin, "fixed array")? != &element_type {
                return Err(Diagnostic::at_origin(
                    "fixed array element has the wrong type",
                    &origin,
                ));
            }
        }
        let ty = Type::fixed_array(element_type.clone(), elements.len());
        Ok(self.push(
            NodeKind::Array {
                element_type,
                elements,
            },
            ty,
            origin,
        ))
    }

    /// Index an array using a statically in-bounds finite integer.
    ///
    /// # Errors
    ///
    /// Returns an error unless the first operand is a fixed array and every
    /// value in the index type lies within the array.
    pub fn index(
        &mut self,
        array: NodeId,
        index: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        let Type::FixedArray { element, len } = self.node_type(array, &origin, "index")? else {
            return Err(Diagnostic::at_origin(
                "index requires a fixed-array operand",
                &origin,
            ));
        };
        let element = element.as_ref().clone();
        let len = *len;
        let Type::Integer(range) = self.node_type(index, &origin, "index")? else {
            return Err(Diagnostic::at_origin(
                "array index requires a finite integer",
                &origin,
            ));
        };
        let len_i32 = usize_to_i32(len, &origin, "index")?;
        if range.min < 0 || range.max >= len_i32 {
            return Err(Diagnostic::at_origin(
                format!(
                    "index range {}..={} is not within fixed array length {len}",
                    range.min, range.max
                ),
                &origin,
            ));
        }
        Ok(self.push(NodeKind::Index { array, index }, element, origin))
    }

    /// Compare two values of the same type for equality.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown nodes or differing operand types.
    pub fn equal(
        &mut self,
        left: NodeId,
        right: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        self.require_same_type(left, right, &origin, "equality")?;
        Ok(self.push(NodeKind::Equal(left, right), Type::Bool, origin))
    }

    /// Compare two finite integers or values of one enum using ordinal order.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported or incompatible operand types.
    pub fn less(
        &mut self,
        left: NodeId,
        right: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        self.require_ordered(left, right, &origin, "less-than")?;
        Ok(self.push(NodeKind::Less(left, right), Type::Bool, origin))
    }

    /// Compare two finite integers or values of one enum using ordinal order.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported or incompatible operand types.
    pub fn less_equal(
        &mut self,
        left: NodeId,
        right: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        self.require_ordered(left, right, &origin, "less-than-or-equal")?;
        Ok(self.push(NodeKind::LessEqual(left, right), Type::Bool, origin))
    }

    /// Negate a Boolean expression.
    ///
    /// # Errors
    ///
    /// Returns an error when the operand is not Boolean.
    pub fn not(&mut self, value: NodeId, origin: Origin) -> Result<NodeId, Diagnostic> {
        self.require_type(value, &Type::Bool, &origin, "not")?;
        Ok(self.push(NodeKind::Not(value), Type::Bool, origin))
    }

    /// Conjoin an explicitly finite list of Boolean expressions.
    ///
    /// The empty conjunction is `true`.
    ///
    /// # Errors
    ///
    /// Returns an error when an operand is not Boolean.
    pub fn fixed_all(&mut self, values: Vec<NodeId>, origin: Origin) -> Result<NodeId, Diagnostic> {
        self.require_all(&values, &Type::Bool, &origin, "fixed all")?;
        Ok(self.push(NodeKind::All(values), Type::Bool, origin))
    }

    /// Disjoin an explicitly finite list of Boolean expressions.
    ///
    /// The empty disjunction is `false`.
    ///
    /// # Errors
    ///
    /// Returns an error when an operand is not Boolean.
    pub fn fixed_any(&mut self, values: Vec<NodeId>, origin: Origin) -> Result<NodeId, Diagnostic> {
        self.require_all(&values, &Type::Bool, &origin, "fixed any")?;
        Ok(self.push(NodeKind::Any(values), Type::Bool, origin))
    }

    /// Select between same-typed values with a Boolean condition.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-Boolean condition or differing branch types.
    pub fn select(
        &mut self,
        condition: NodeId,
        when_true: NodeId,
        when_false: NodeId,
        origin: Origin,
    ) -> Result<NodeId, Diagnostic> {
        self.require_type(condition, &Type::Bool, &origin, "conditional")?;
        let ty = self
            .require_same_type(when_true, when_false, &origin, "conditional")?
            .clone();
        Ok(self.push(
            NodeKind::Select {
                condition,
                when_true,
                when_false,
            },
            ty,
            origin,
        ))
    }

    /// Sum an explicitly finite list of bounded integer expressions.
    ///
    /// The empty sum is the singleton integer type `0..=0`.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-integer operand or if the resulting bounds
    /// overflow `i32`.
    pub fn fixed_sum(&mut self, values: Vec<NodeId>, origin: Origin) -> Result<NodeId, Diagnostic> {
        let mut min = 0_i32;
        let mut max = 0_i32;
        for value in &values {
            let Type::Integer(range) = self.node_type(*value, &origin, "fixed sum")? else {
                return Err(Diagnostic::at_origin(
                    "fixed sum requires finite integer operands",
                    &origin,
                ));
            };
            min = min.checked_add(range.min).ok_or_else(|| {
                Diagnostic::at_origin("fixed-sum lower bound overflows i32", &origin)
            })?;
            max = max.checked_add(range.max).ok_or_else(|| {
                Diagnostic::at_origin("fixed-sum upper bound overflows i32", &origin)
            })?;
        }
        let range = IntRange { min, max };
        Ok(self.push(
            NodeKind::Sum { values, range },
            Type::Integer(range),
            origin,
        ))
    }

    /// Reject a request for behavior outside the portable dialect.
    ///
    /// This is the authoring boundary for code that discovers it would need an
    /// opaque Rust capability. The caller must model the behavior explicitly or
    /// stop; there is no callback escape hatch.
    ///
    /// # Errors
    ///
    /// Always returns a source-oriented unsupported-feature diagnostic.
    pub fn reject_unsupported(
        &self,
        feature: UnsupportedFeature,
        origin: &Origin,
    ) -> Result<(), Diagnostic> {
        Err(Diagnostic::at_origin(
            format!(
                "{} is not supported by the pure finite formal dialect",
                feature.description()
            ),
            origin,
        ))
    }

    /// Lower one rooted expression to independently executable Weavy code.
    ///
    /// # Errors
    ///
    /// Returns a source-oriented error if the root or any dependency is absent.
    pub fn lower(&self, root: NodeId) -> Result<FormalProgram, Diagnostic> {
        let root_node = self.nodes.get(root.0).ok_or_else(|| {
            Diagnostic::at_origin(format!("root node {} is absent", root.0), &unknown_origin())
        })?;
        let mut work = vec![(root, false)];
        let mut ops = Vec::new();
        let mut input_origins = BTreeMap::new();
        while let Some((node_id, emit)) = work.pop() {
            let node = self.nodes.get(node_id.0).ok_or_else(|| {
                Diagnostic::at_origin(
                    format!("node {} is absent while lowering", node_id.0),
                    &root_node.origin,
                )
            })?;
            if emit {
                if let NodeKind::Input(name) = &node.kind {
                    input_origins
                        .entry(name.clone())
                        .or_insert_with(|| node.origin.clone());
                }
                ops.push(LocatedOp {
                    op: Op::from_node(node),
                    origin: node.origin.clone(),
                });
                continue;
            }
            work.push((node_id, true));
            let children = node.kind.children();
            for child in children.into_iter().rev() {
                if self.nodes.get(child.0).is_none() {
                    return Err(Diagnostic::at_origin(
                        format!("node {} refers to absent node {}", node_id.0, child.0),
                        &node.origin,
                    ));
                }
                work.push((child, false));
            }
        }
        let mut inputs = BTreeMap::new();
        for name in input_origins.keys() {
            let Some(ty) = self.inputs.get(name) else {
                return Err(Diagnostic::at_origin(
                    format!("lowered input {name:?} has no declaration"),
                    &root_node.origin,
                ));
            };
            inputs.insert(name.clone(), ty.clone());
        }
        Ok(FormalProgram {
            lowered: Lowered::new(ops),
            inputs,
            input_origins,
            output_type: root_node.ty.clone(),
        })
    }

    fn push(&mut self, kind: NodeKind, ty: Type, origin: Origin) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node { kind, ty, origin });
        id
    }

    fn node_type(
        &self,
        node: NodeId,
        origin: &Origin,
        operation: &str,
    ) -> Result<&Type, Diagnostic> {
        self.nodes.get(node.0).map(|node| &node.ty).ok_or_else(|| {
            Diagnostic::at_origin(
                format!("{operation} refers to absent node {}", node.0),
                origin,
            )
        })
    }

    fn require_type(
        &self,
        node: NodeId,
        expected: &Type,
        origin: &Origin,
        operation: &str,
    ) -> Result<(), Diagnostic> {
        if self.node_type(node, origin, operation)? == expected {
            Ok(())
        } else {
            Err(Diagnostic::at_origin(
                format!("{operation} operand has the wrong type"),
                origin,
            ))
        }
    }

    fn require_all(
        &self,
        nodes: &[NodeId],
        expected: &Type,
        origin: &Origin,
        operation: &str,
    ) -> Result<(), Diagnostic> {
        for node in nodes {
            self.require_type(*node, expected, origin, operation)?;
        }
        Ok(())
    }

    fn require_same_type(
        &self,
        left: NodeId,
        right: NodeId,
        origin: &Origin,
        operation: &str,
    ) -> Result<&Type, Diagnostic> {
        let left_type = self.node_type(left, origin, operation)?;
        let right_type = self.node_type(right, origin, operation)?;
        if left_type == right_type {
            Ok(left_type)
        } else {
            Err(Diagnostic::at_origin(
                format!("{operation} operands have different types"),
                origin,
            ))
        }
    }

    fn require_ordered(
        &self,
        left: NodeId,
        right: NodeId,
        origin: &Origin,
        operation: &str,
    ) -> Result<(), Diagnostic> {
        let ty = self.require_same_type(left, right, origin, operation)?;
        if matches!(ty, Type::Integer(_) | Type::Enumeration(_)) {
            Ok(())
        } else {
            Err(Diagnostic::at_origin(
                format!("{operation} is defined only for finite integers and enums"),
                origin,
            ))
        }
    }
}

#[derive(Clone, Debug)]
struct LocatedOp {
    op: Op,
    origin: Origin,
}

#[derive(Clone, Debug)]
enum Op {
    Push(Value),
    LoadInput(String),
    Enumeration {
        type_name: String,
        ordinal: usize,
    },
    EnumerationOrdinal {
        variant_count: usize,
    },
    Record {
        type_name: String,
        field_count: usize,
    },
    Field(usize),
    Array {
        element_type: Type,
        len: usize,
    },
    Index,
    Equal,
    Less,
    LessEqual,
    Not,
    All(usize),
    Any(usize),
    Select,
    Sum {
        len: usize,
        range: IntRange,
    },
}

impl Op {
    fn from_node(node: &Node) -> Self {
        match &node.kind {
            NodeKind::Constant(value) => Self::Push(value.clone()),
            NodeKind::Input(name) => Self::LoadInput(name.clone()),
            NodeKind::Enumeration { schema, ordinal } => Self::Enumeration {
                type_name: schema.name.clone(),
                ordinal: *ordinal,
            },
            NodeKind::EnumerationOrdinal { variant_count, .. } => Self::EnumerationOrdinal {
                variant_count: *variant_count,
            },
            NodeKind::Record { schema, fields } => Self::Record {
                type_name: schema.name.clone(),
                field_count: fields.len(),
            },
            NodeKind::Field { index, .. } => Self::Field(*index),
            NodeKind::Array {
                element_type,
                elements,
            } => Self::Array {
                element_type: element_type.clone(),
                len: elements.len(),
            },
            NodeKind::Index { .. } => Self::Index,
            NodeKind::Equal(..) => Self::Equal,
            NodeKind::Less(..) => Self::Less,
            NodeKind::LessEqual(..) => Self::LessEqual,
            NodeKind::Not(..) => Self::Not,
            NodeKind::All(values) => Self::All(values.len()),
            NodeKind::Any(values) => Self::Any(values.len()),
            NodeKind::Select { .. } => Self::Select,
            NodeKind::Sum { values, range } => Self::Sum {
                len: values.len(),
                range: *range,
            },
        }
    }
}

/// A validated, lowered formal computation.
#[derive(Clone, Debug)]
pub struct FormalProgram {
    lowered: Lowered<(), LocatedOp>,
    inputs: BTreeMap<String, Type>,
    input_origins: BTreeMap<String, Origin>,
    output_type: Type,
}

impl FormalProgram {
    /// Return the exact required runtime input types.
    #[must_use]
    pub const fn inputs(&self) -> &BTreeMap<String, Type> {
        &self.inputs
    }

    /// Return the computation's output type.
    #[must_use]
    pub const fn output_type(&self) -> &Type {
        &self.output_type
    }

    /// Return origins in lowered execution order.
    #[must_use]
    pub fn origins(&self) -> impl ExactSizeIterator<Item = &Origin> {
        self.lowered.program.iter().map(|op| &op.origin)
    }

    /// Evaluate with concrete, exactly typed inputs.
    ///
    /// # Errors
    ///
    /// Returns a source-oriented diagnostic for missing, extra, or ill-typed
    /// inputs and for any violated interpreter invariant.
    pub fn evaluate(&self, inputs: &BTreeMap<String, Value>) -> Result<Value, Diagnostic> {
        let fallback_origin = self
            .lowered
            .program
            .last()
            .map_or_else(unknown_origin, |op| op.origin.clone());
        for (name, ty) in &self.inputs {
            let input_origin = self.input_origins.get(name).unwrap_or(&fallback_origin);
            let value = inputs.get(name).ok_or_else(|| {
                Diagnostic::at_origin(format!("required input {name:?} is missing"), input_origin)
            })?;
            if !ty.accepts(value) {
                return Err(Diagnostic::at_origin(
                    format!("input {name:?} does not belong to its declared finite type"),
                    input_origin,
                ));
            }
        }
        if let Some(extra) = inputs.keys().find(|name| !self.inputs.contains_key(*name)) {
            return Err(Diagnostic::at_origin(
                format!("undeclared input {extra:?} was supplied"),
                &fallback_origin,
            ));
        }

        let mut evaluator = Evaluator {
            inputs,
            stack: Vec::new(),
            instruction: 0,
        };
        match weavy::run(&self.lowered, &mut evaluator) {
            Ok(()) => {}
            Err(RunError::Step(error)) => return Err(error),
            Err(RunError::MissingBlock(())) => {
                return Err(Diagnostic::at_origin(
                    "lowered formal program refers to a missing Weavy block",
                    &fallback_origin,
                ));
            }
        }
        if evaluator.stack.len() != 1 {
            return Err(Diagnostic::at_origin(
                format!(
                    "formal evaluation ended with {} values instead of one",
                    evaluator.stack.len()
                ),
                &fallback_origin,
            ));
        }
        let Some(value) = evaluator.stack.pop() else {
            return Err(Diagnostic::at_origin(
                "formal evaluation lost its final value",
                &fallback_origin,
            ));
        };
        if !self.output_type.accepts(&value) {
            return Err(Diagnostic::at_origin(
                "formal evaluation produced a value outside its output type",
                &fallback_origin,
            ));
        }
        Ok(value)
    }
}

struct Evaluator<'inputs> {
    inputs: &'inputs BTreeMap<String, Value>,
    stack: Vec<Value>,
    instruction: usize,
}

impl Evaluator<'_> {
    fn fail(&self, message: impl Into<String>, origin: &Origin) -> Diagnostic {
        Diagnostic::at_instruction(message, origin, self.instruction)
    }

    fn pop(&mut self, origin: &Origin) -> Result<Value, Diagnostic> {
        self.stack
            .pop()
            .ok_or_else(|| self.fail("formal evaluator stack underflow", origin))
    }

    fn pop_n(&mut self, count: usize, origin: &Origin) -> Result<Vec<Value>, Diagnostic> {
        if count > self.stack.len() {
            return Err(self.fail(
                format!(
                    "formal evaluator needs {count} operands but has {}",
                    self.stack.len()
                ),
                origin,
            ));
        }
        Ok(self.stack.split_off(self.stack.len() - count))
    }

    fn boolean(&self, value: &Value, origin: &Origin) -> Result<bool, Diagnostic> {
        if let Value::Bool(value) = value {
            Ok(*value)
        } else {
            Err(self.fail("formal evaluator expected a Boolean", origin))
        }
    }

    fn ordinal(&self, value: &Value, origin: &Origin) -> Result<i32, Diagnostic> {
        match value {
            Value::Integer(value) => Ok(*value),
            Value::Enumeration { ordinal, .. } => i32::try_from(*ordinal)
                .map_err(|_| self.fail("enum ordinal does not fit i32", origin)),
            _ => Err(self.fail("formal evaluator expected an ordered scalar", origin)),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one exhaustive opcode dispatch keeps the formal semantics auditable"
    )]
    fn execute(&mut self, located: &LocatedOp) -> Result<(), Diagnostic> {
        let origin = &located.origin;
        match &located.op {
            Op::Push(value) => self.stack.push(value.clone()),
            Op::LoadInput(name) => {
                let value = self.inputs.get(name).cloned().ok_or_else(|| {
                    self.fail(format!("required input {name:?} disappeared"), origin)
                })?;
                self.stack.push(value);
            }
            Op::Enumeration { type_name, ordinal } => {
                self.stack.push(Value::Enumeration {
                    type_name: type_name.clone(),
                    ordinal: *ordinal,
                });
            }
            Op::EnumerationOrdinal { variant_count } => {
                let value = self.pop(origin)?;
                let Value::Enumeration { ordinal, .. } = value else {
                    return Err(self.fail("enum ordinal received a non-enum value", origin));
                };
                if ordinal >= *variant_count {
                    return Err(self.fail("enum ordinal is outside its schema", origin));
                }
                let ordinal = i32::try_from(ordinal)
                    .map_err(|_| self.fail("enum ordinal does not fit i32", origin))?;
                self.stack.push(Value::Integer(ordinal));
            }
            Op::Record {
                type_name,
                field_count,
            } => {
                let fields = self.pop_n(*field_count, origin)?;
                self.stack.push(Value::Record {
                    type_name: type_name.clone(),
                    fields,
                });
            }
            Op::Field(index) => {
                let value = self.pop(origin)?;
                let Value::Record { fields, .. } = value else {
                    return Err(self.fail("field projection received a non-record", origin));
                };
                let field = fields.get(*index).cloned().ok_or_else(|| {
                    self.fail(format!("record field index {index} is absent"), origin)
                })?;
                self.stack.push(field);
            }
            Op::Array { element_type, len } => {
                let values = self.pop_n(*len, origin)?;
                if values.iter().any(|value| !element_type.accepts(value)) {
                    return Err(self.fail("fixed array contains an ill-typed element", origin));
                }
                self.stack.push(Value::FixedArray(values));
            }
            Op::Index => {
                let index = self.pop(origin)?;
                let array = self.pop(origin)?;
                let Value::Integer(index) = index else {
                    return Err(self.fail("fixed index received a non-integer index", origin));
                };
                let Ok(index) = usize::try_from(index) else {
                    return Err(self.fail("fixed index received a negative index", origin));
                };
                let Value::FixedArray(values) = array else {
                    return Err(self.fail("fixed index received a non-array", origin));
                };
                let value = values.get(index).cloned().ok_or_else(|| {
                    self.fail(format!("fixed index {index} is out of bounds"), origin)
                })?;
                self.stack.push(value);
            }
            Op::Equal => {
                let right = self.pop(origin)?;
                let left = self.pop(origin)?;
                self.stack.push(Value::Bool(left == right));
            }
            Op::Less | Op::LessEqual => {
                let right = self.pop(origin)?;
                let left = self.pop(origin)?;
                let left = self.ordinal(&left, origin)?;
                let right = self.ordinal(&right, origin)?;
                self.stack
                    .push(Value::Bool(if matches!(located.op, Op::Less) {
                        left < right
                    } else {
                        left <= right
                    }));
            }
            Op::Not => {
                let value = self.pop(origin)?;
                let value = self.boolean(&value, origin)?;
                self.stack.push(Value::Bool(!value));
            }
            Op::All(count) | Op::Any(count) => {
                let values = self.pop_n(*count, origin)?;
                let mut booleans = Vec::with_capacity(values.len());
                for value in values {
                    booleans.push(self.boolean(&value, origin)?);
                }
                let result = if matches!(located.op, Op::All(_)) {
                    booleans.into_iter().all(|value| value)
                } else {
                    booleans.into_iter().any(|value| value)
                };
                self.stack.push(Value::Bool(result));
            }
            Op::Select => {
                let when_false = self.pop(origin)?;
                let when_true = self.pop(origin)?;
                let condition = self.pop(origin)?;
                let condition = self.boolean(&condition, origin)?;
                self.stack
                    .push(if condition { when_true } else { when_false });
            }
            Op::Sum { len, range } => {
                let values = self.pop_n(*len, origin)?;
                let mut total = 0_i32;
                for value in values {
                    let Value::Integer(value) = value else {
                        return Err(self.fail("fixed sum received a non-integer", origin));
                    };
                    total = total
                        .checked_add(value)
                        .ok_or_else(|| self.fail("fixed sum overflowed i32", origin))?;
                }
                if !range.contains(total) {
                    return Err(self.fail("fixed sum escaped its inferred range", origin));
                }
                self.stack.push(Value::Integer(total));
            }
        }
        Ok(())
    }
}

impl<'program> Step<'program, (), LocatedOp> for Evaluator<'_> {
    type Error = Diagnostic;
    type Continuation = ();

    fn step(
        &mut self,
        op: &'program LocatedOp,
    ) -> Result<Control<'program, (), LocatedOp>, Self::Error> {
        self.execute(op)?;
        self.instruction += 1;
        Ok(Control::Continue)
    }
}

fn ensure_name(kind: &str, name: &str) -> Result<(), SchemaError> {
    if name.trim().is_empty() {
        Err(SchemaError::new(format!("{kind} name is empty")))
    } else {
        Ok(())
    }
}

fn usize_to_i32(value: usize, origin: &Origin, operation: &str) -> Result<i32, Diagnostic> {
    i32::try_from(value).map_err(|_| {
        Diagnostic::at_origin(
            format!("{operation} static bound {value} does not fit i32"),
            origin,
        )
    })
}

fn unknown_origin() -> Origin {
    Origin::new(
        "INTERNAL",
        SourceRef::new("<formal-graph>", 0, "unavailable origin"),
        ComputationKind::Property,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(rule_id: &str, computation: ComputationKind) -> Origin {
        Origin::new(
            rule_id,
            SourceRef::new("docs/main.typ", 100, "test clause"),
            computation,
        )
    }

    fn small_int() -> Type {
        Type::Integer(IntRange::new(0, 3).unwrap())
    }

    #[test]
    fn expression_eval_covers_supported_intrinsics() {
        let mut graph = GraphBuilder::new();
        let property = origin("P-FOLLOW-SUIT", ComputationKind::Property);
        let phase = EnumType::new("Phase", ["Bid", "Play", "Score"]).unwrap();
        let play = graph
            .enumeration(phase.clone(), "Play", property.clone())
            .unwrap();
        let phase_ordinal = graph.enumeration_ordinal(play, property.clone()).unwrap();
        let phase_limit = graph
            .constant(
                Type::Integer(IntRange::new(0, 2).unwrap()),
                Value::Integer(2),
                property.clone(),
            )
            .unwrap();
        let one = graph
            .constant(small_int(), Value::Integer(1), property.clone())
            .unwrap();
        let two = graph
            .constant(small_int(), Value::Integer(2), property.clone())
            .unwrap();
        let three = graph
            .constant(small_int(), Value::Integer(3), property.clone())
            .unwrap();
        let player = RecordType::new(
            "Player",
            [
                FieldType::new("bid", small_int()).unwrap(),
                FieldType::new("phase", Type::Enumeration(phase)).unwrap(),
            ],
        )
        .unwrap();
        let record = graph
            .record(player, vec![one, play], property.clone())
            .unwrap();
        let bid = graph.field(record, "bid", property.clone()).unwrap();
        let array = graph
            .fixed_array(small_int(), vec![one, two, three], property.clone())
            .unwrap();
        let index_type = Type::Integer(IntRange::new(0, 2).unwrap());
        let index = graph
            .input("chosen-index", index_type, property.clone())
            .unwrap();
        let selected = graph.index(array, index, property.clone()).unwrap();
        let bid_is_one = graph.equal(bid, one, property.clone()).unwrap();
        let selected_before_three = graph.less(selected, three, property.clone()).unwrap();
        let phase_at_most_play = graph
            .less_equal(phase_ordinal, phase_limit, property.clone())
            .unwrap();
        let false_value = graph.not(bid_is_one, property.clone()).unwrap();
        let some_false = graph
            .fixed_any(vec![false_value, selected_before_three], property.clone())
            .unwrap();
        let condition = graph
            .fixed_all(
                vec![
                    bid_is_one,
                    selected_before_three,
                    phase_at_most_play,
                    some_false,
                ],
                property.clone(),
            )
            .unwrap();
        let total = graph
            .fixed_sum(vec![bid, selected, phase_ordinal], property.clone())
            .unwrap();
        let fallback = graph
            .constant(
                Type::Integer(IntRange::new(0, 8).unwrap()),
                Value::Integer(0),
                property.clone(),
            )
            .unwrap();
        // The sum's inferred type is 0..=8, matching the fallback.
        let result = graph.select(condition, total, fallback, property).unwrap();
        let program = graph.lower(result).unwrap();
        let inputs = BTreeMap::from([("chosen-index".to_owned(), Value::Integer(1))]);

        assert_eq!(program.evaluate(&inputs).unwrap(), Value::Integer(4));
        assert_eq!(
            program.output_type(),
            &Type::Integer(IntRange::new(0, 8).unwrap())
        );
    }

    #[test]
    fn lowering_validation_rejects_nonportable_and_ill_typed_graphs() {
        let mut graph = GraphBuilder::new();
        let transition = origin("T-CHANCE", ComputationKind::Transition);
        for feature in [
            UnsupportedFeature::OpaqueHostCall,
            UnsupportedFeature::Effect,
            UnsupportedFeature::DynamicAllocation,
            UnsupportedFeature::UnboundedIteration,
            UnsupportedFeature::HiddenRandomness,
            UnsupportedFeature::UnsupportedArithmetic,
        ] {
            let error = graph.reject_unsupported(feature, &transition).unwrap_err();
            assert_eq!(error.origin().rule_id(), "T-CHANCE");
            assert!(error.message().contains(feature.description()));
        }

        let boolean = graph
            .constant(Type::Bool, Value::Bool(true), transition.clone())
            .unwrap();
        let integer = graph
            .constant(small_int(), Value::Integer(1), transition.clone())
            .unwrap();
        let mismatch = graph
            .equal(boolean, integer, transition.clone())
            .unwrap_err();
        assert!(mismatch.message().contains("different types"));

        let array = graph
            .fixed_array(small_int(), vec![integer], transition.clone())
            .unwrap();
        let wide_index = graph
            .input(
                "wide-index",
                Type::Integer(IntRange::new(0, 1).unwrap()),
                transition.clone(),
            )
            .unwrap();
        let bounds = graph.index(array, wide_index, transition).unwrap_err();
        assert!(bounds.message().contains("not within fixed array length"));
    }

    #[test]
    fn origin_mapping_preserves_all_computation_roles_and_runtime_location() {
        let mut graph = GraphBuilder::new();
        let kinds = [
            ComputationKind::State,
            ComputationKind::Observation,
            ComputationKind::LegalAction,
            ComputationKind::Transition,
            ComputationKind::Scoring,
            ComputationKind::Property,
        ];
        let mut nodes = Vec::new();
        for (index, kind) in kinds.into_iter().enumerate() {
            nodes.push(
                graph
                    .constant(
                        Type::Bool,
                        Value::Bool(true),
                        origin(&format!("ROLE-{index}"), kind),
                    )
                    .unwrap(),
            );
        }
        let root_origin = Origin::new(
            "P-ALL-ROLES",
            SourceRef::new("docs/main.typ", 321, "all roles"),
            ComputationKind::Property,
        );
        let root = graph.fixed_all(nodes, root_origin.clone()).unwrap();
        let mut program = graph.lower(root).unwrap();
        let present: BTreeSet<ComputationKind> =
            program.origins().map(Origin::computation).collect();
        assert_eq!(present, kinds.into_iter().collect());

        // Corrupt a private lowered instruction to prove runtime diagnostics do
        // not lose the source of a backend/interpreter invariant failure.
        program.lowered.program[0].op = Op::Not;
        let error = program.evaluate(&BTreeMap::new()).unwrap_err();
        assert_eq!(error.origin().rule_id(), "ROLE-0");
        assert_eq!(error.origin().source().line(), 100);
        assert_eq!(error.instruction(), Some(0));
        assert!(error.to_string().contains("docs/main.typ:100"));
    }
}
