use std::collections::{HashMap, HashSet};

use toml_dom::{
    Document, DocumentItem, EnumChoice, Quantity, QuantityMagnitude, QuantitySeparator, Table,
    Value, ValueNode,
};

use crate::ByteRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefinitionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Key,
    Table,
    ArrayOfTables,
    Enum,
    EnumMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    String,
    Integer,
    Float,
    Boolean,
    OffsetDateTime,
    LocalDateTime,
    LocalDate,
    LocalTime,
    Array,
    Table,
    Quantity,
    EnumReference,
    EnumDefinition,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub id: DefinitionId,
    pub name: String,
    pub path: Vec<String>,
    pub kind: DefinitionKind,
    pub value_type: ValueType,
    pub range: ByteRange,
    pub selection_range: ByteRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Path,
    EnumMember,
    Inheritance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub source_path: Vec<String>,
    pub target_path: Vec<String>,
    pub range: ByteRange,
    pub target: Option<DefinitionId>,
    pub resolved_target: Option<DefinitionId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuantityOccurrence {
    pub path: Vec<String>,
    pub range: ByteRange,
    pub magnitude: QuantityMagnitude,
    pub unit: String,
    pub separator: Option<QuantitySeparator>,
    pub super_unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritanceEdge {
    pub child_path: Vec<String>,
    pub parent_path: Vec<String>,
    pub range: ByteRange,
    pub parent: Option<DefinitionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleKind {
    PathReference,
    Inheritance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCycle {
    pub kind: CycleKind,
    pub paths: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SemanticIndex {
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
    pub quantities: Vec<QuantityOccurrence>,
    pub inheritance: Vec<InheritanceEdge>,
    pub cycles: Vec<SemanticCycle>,
}

impl SemanticIndex {
    pub fn build(source: &str, document: &Document) -> Self {
        let mut builder = Builder::new(source, document);
        builder.collect();
        builder.finish()
    }

    pub fn definition(&self, id: DefinitionId) -> Option<&Definition> {
        self.definitions.get(id.0)
    }

    pub fn definitions_at_path<'a>(
        &'a self,
        path: &[String],
    ) -> impl Iterator<Item = &'a Definition> + 'a {
        let path = path.to_vec();
        self.definitions
            .iter()
            .filter(move |definition| definition.path == path)
    }

    pub fn resolved_reference(&self, reference: &Reference) -> Option<&Definition> {
        self.definition(reference.resolved_target?)
    }

    /// Return all inherited parent definitions in declaration priority order.
    /// Transitive parents follow their direct child; repeated parents and
    /// cycles are omitted.
    pub fn inheritance_chain(&self, child_path: &[String]) -> Vec<&Definition> {
        fn collect<'a>(
            index: &'a SemanticIndex,
            child: &[String],
            visited: &mut HashSet<Vec<String>>,
            result: &mut Vec<&'a Definition>,
        ) {
            for edge in index
                .inheritance
                .iter()
                .filter(|edge| edge.child_path == child)
            {
                if !visited.insert(edge.parent_path.clone()) {
                    continue;
                }
                if let Some(parent) = edge.parent.and_then(|id| index.definition(id)) {
                    result.push(parent);
                    collect(index, &edge.parent_path, visited, result);
                }
            }
        }

        let mut result = Vec::new();
        collect(self, child_path, &mut HashSet::new(), &mut result);
        result
    }
}

struct Builder<'a> {
    source: &'a str,
    document: &'a Document,
    index: SemanticIndex,
    cursor: usize,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str, document: &'a Document) -> Self {
        Self {
            source,
            document,
            index: SemanticIndex::default(),
            cursor: 0,
        }
    }

    fn collect(&mut self) {
        for item in self.document.items() {
            match item {
                DocumentItem::Entry { node, path } => self.collect_entry(node, path),
                DocumentItem::Section(section) => {
                    let Some(range) = self.locate(&section.raw) else {
                        continue;
                    };
                    self.add_definition(
                        section.path.clone(),
                        if section.is_array {
                            DefinitionKind::ArrayOfTables
                        } else {
                            DefinitionKind::Table
                        },
                        ValueType::Table,
                        range,
                        range,
                    );
                    for parent in &section.parents {
                        self.index.inheritance.push(InheritanceEdge {
                            child_path: section.path.clone(),
                            parent_path: parent.clone(),
                            range,
                            parent: None,
                        });
                    }
                }
                DocumentItem::Eof(_) => {}
            }
        }
    }

    fn collect_entry(&mut self, node: &toml_dom::EntryNode, path: &[String]) {
        let Some(key_range) = self.locate(&node.raw_key) else {
            return;
        };
        let value_start = key_range.end + node.pre_eq.len() + 1 + node.post_eq.len();
        let value_range = ByteRange {
            start: value_start,
            end: value_start + node_len(&node.node),
        };
        let semantic_value = value_at_path(self.document.root(), path);
        let (kind, value_type) = match semantic_value {
            Some(Value::EnumDefinition(_)) => (DefinitionKind::Enum, ValueType::EnumDefinition),
            value => (
                DefinitionKind::Key,
                value.map_or(ValueType::Unknown, value_type),
            ),
        };
        self.add_definition(
            path.to_vec(),
            kind,
            value_type,
            ByteRange {
                start: key_range.start,
                end: value_range.end,
            },
            key_range,
        );

        if let Some(Value::EnumDefinition(definition)) = semantic_value {
            self.collect_enum_members(path, definition, &node.node, value_start);
        }
        self.collect_node(path, &node.node, value_start);
        self.cursor = self.cursor.max(value_range.end);
    }

    fn collect_enum_members(
        &mut self,
        enum_path: &[String],
        definition: &toml_dom::EnumDefinition,
        node: &ValueNode,
        start: usize,
    ) {
        let ValueNode::Array(array) = node else {
            return;
        };
        let mut offset = start + array.open.len();
        for (choice, element) in definition.choices.iter().zip(&array.elements) {
            offset += element.leading.len();
            let range = ByteRange {
                start: offset,
                end: offset + node_len(&element.node),
            };
            let (name, choice_type) = match choice {
                EnumChoice::Symbol(symbol) => (symbol.clone(), ValueType::EnumReference),
                EnumChoice::Value(value) => (
                    self.source[range.start..range.end].trim().to_owned(),
                    value_type(value),
                ),
            };
            let mut path = enum_path.to_vec();
            path.push(name);
            self.add_definition(path, DefinitionKind::EnumMember, choice_type, range, range);
            offset += node_len(&element.node) + element.trailing.len();
            offset += element.comma.as_ref().map_or(0, String::len);
        }
    }

    fn collect_node(&mut self, path: &[String], node: &ValueNode, start: usize) {
        match node {
            ValueNode::Scalar { raw, value } => {
                let range = ByteRange {
                    start,
                    end: start + raw.as_ref().map_or(0, String::len),
                };
                if let Some(raw) = raw {
                    if let Some(target_path) = parse_path_reference(raw) {
                        self.index.references.push(Reference {
                            kind: ReferenceKind::Path,
                            source_path: path.to_vec(),
                            target_path,
                            range,
                            target: None,
                            resolved_target: None,
                        });
                        return;
                    }
                }
                match value {
                    Value::Enum(reference) => {
                        let mut target_path = reference.enum_path.clone();
                        target_path.push(reference.symbol.clone());
                        self.index.references.push(Reference {
                            kind: ReferenceKind::EnumMember,
                            source_path: path.to_vec(),
                            target_path,
                            range,
                            target: None,
                            resolved_target: None,
                        });
                    }
                    Value::Quantity(quantity) => {
                        self.add_quantity(path, range, quantity);
                    }
                    _ => {}
                }
            }
            ValueNode::Array(array) => {
                let mut offset = start + array.open.len();
                for (index, element) in array.elements.iter().enumerate() {
                    offset += element.leading.len();
                    let mut element_path = path.to_vec();
                    element_path.push(index.to_string());
                    self.collect_node(&element_path, &element.node, offset);
                    offset += node_len(&element.node) + element.trailing.len();
                    offset += element.comma.as_ref().map_or(0, String::len);
                }
            }
            ValueNode::InlineTable(table) => {
                let mut offset = start + table.open.len();
                for entry in &table.entries {
                    offset += entry.leading.len();
                    offset += entry.raw_key.len() + entry.pre_eq.len() + 1 + entry.post_eq.len();
                    let mut entry_path = path.to_vec();
                    entry_path.push(entry.raw_key.trim().to_owned());
                    self.collect_node(&entry_path, &entry.node, offset);
                    offset += node_len(&entry.node) + entry.trailing.len();
                    offset += entry.comma.as_ref().map_or(0, String::len);
                }
            }
            ValueNode::EnumSymbol { .. } => {}
        }
    }

    fn add_quantity(&mut self, path: &[String], range: ByteRange, quantity: &Quantity) {
        self.index.quantities.push(QuantityOccurrence {
            path: path.to_vec(),
            range,
            magnitude: quantity.magnitude,
            unit: quantity.unit.clone(),
            separator: quantity.separator,
            super_unit: quantity.super_unit.clone(),
        });
    }

    fn add_definition(
        &mut self,
        path: Vec<String>,
        kind: DefinitionKind,
        value_type: ValueType,
        range: ByteRange,
        selection_range: ByteRange,
    ) -> DefinitionId {
        let id = DefinitionId(self.index.definitions.len());
        let name = path.last().cloned().unwrap_or_default();
        self.index.definitions.push(Definition {
            id,
            name,
            path,
            kind,
            value_type,
            range,
            selection_range,
        });
        id
    }

    fn locate(&mut self, needle: &str) -> Option<ByteRange> {
        let relative = self.source.get(self.cursor..)?.find(needle)?;
        let start = self.cursor + relative;
        let end = start + needle.len();
        self.cursor = end;
        Some(ByteRange { start, end })
    }

    fn finish(mut self) -> SemanticIndex {
        let definitions = self
            .index
            .definitions
            .iter()
            .map(|definition| (definition.path.clone(), definition.id))
            .collect::<HashMap<_, _>>();
        for reference in &mut self.index.references {
            reference.target = definitions.get(&reference.target_path).copied();
        }
        let path_targets = self
            .index
            .references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::Path)
            .map(|reference| (reference.source_path.clone(), reference.target_path.clone()))
            .collect::<HashMap<_, _>>();
        for reference in &mut self.index.references {
            if reference.kind != ReferenceKind::Path {
                reference.resolved_target = reference.target;
                continue;
            }
            let mut current = reference.target_path.clone();
            let mut visited = HashSet::new();
            while visited.insert(current.clone()) {
                let Some(next) = path_targets.get(&current) else {
                    break;
                };
                current = next.clone();
            }
            reference.resolved_target = definitions.get(&current).copied();
        }
        for edge in &mut self.index.inheritance {
            edge.parent = definitions.get(&edge.parent_path).copied();
        }

        let path_edges = self
            .index
            .references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::Path)
            .map(|reference| (reference.source_path.clone(), reference.target_path.clone()))
            .collect::<Vec<_>>();
        self.index
            .cycles
            .extend(detect_cycles(CycleKind::PathReference, &path_edges));
        let inheritance_edges = self
            .index
            .inheritance
            .iter()
            .map(|edge| (edge.child_path.clone(), edge.parent_path.clone()))
            .collect::<Vec<_>>();
        self.index
            .cycles
            .extend(detect_cycles(CycleKind::Inheritance, &inheritance_edges));
        self.index
    }
}

fn value_at_path<'a>(root: &'a Table, path: &[String]) -> Option<&'a Value> {
    let mut value = root.get(path.first()?)?;
    for segment in &path[1..] {
        value = match value {
            Value::Table(table) => table.get(segment)?,
            Value::Array(array) => array.iter().nth(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn value_type(value: &Value) -> ValueType {
    match value {
        Value::String(_) => ValueType::String,
        Value::Integer(_) => ValueType::Integer,
        Value::Float(_) => ValueType::Float,
        Value::Boolean(_) => ValueType::Boolean,
        Value::OffsetDateTime(_) => ValueType::OffsetDateTime,
        Value::LocalDateTime(_) => ValueType::LocalDateTime,
        Value::LocalDate(_) => ValueType::LocalDate,
        Value::LocalTime(_) => ValueType::LocalTime,
        Value::Array(_) => ValueType::Array,
        Value::Table(_) => ValueType::Table,
        Value::Quantity(_) => ValueType::Quantity,
        Value::Enum(_) => ValueType::EnumReference,
        Value::EnumDefinition(_) => ValueType::EnumDefinition,
        _ => ValueType::Unknown,
    }
}

fn node_len(node: &ValueNode) -> usize {
    match node {
        ValueNode::Scalar { raw, .. } => raw.as_ref().map_or(0, String::len),
        ValueNode::EnumSymbol { raw, .. } => raw.len(),
        ValueNode::Array(array) => {
            array.open.len()
                + array
                    .elements
                    .iter()
                    .map(|element| {
                        element.leading.len()
                            + node_len(&element.node)
                            + element.trailing.len()
                            + element.comma.as_ref().map_or(0, String::len)
                    })
                    .sum::<usize>()
                + array.close.len()
        }
        ValueNode::InlineTable(table) => {
            table.open.len()
                + table
                    .entries
                    .iter()
                    .map(|entry| {
                        entry.leading.len()
                            + entry.raw_key.len()
                            + entry.pre_eq.len()
                            + 1
                            + entry.post_eq.len()
                            + node_len(&entry.node)
                            + entry.trailing.len()
                            + entry.comma.as_ref().map_or(0, String::len)
                    })
                    .sum::<usize>()
                + table.close.len()
        }
    }
}

fn parse_path_reference(raw: &str) -> Option<Vec<String>> {
    let segments = raw.split('.').collect::<Vec<_>>();
    if segments.len() < 2
        || segments.iter().any(|segment| {
            let mut chars = segment.chars();
            !chars
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        })
    {
        return None;
    }
    Some(segments.into_iter().map(str::to_owned).collect())
}

fn detect_cycles(kind: CycleKind, edges: &[(Vec<String>, Vec<String>)]) -> Vec<SemanticCycle> {
    let mut graph: HashMap<Vec<String>, Vec<Vec<String>>> = HashMap::new();
    for (source, target) in edges {
        graph
            .entry(source.clone())
            .or_default()
            .push(target.clone());
    }
    let mut active = HashSet::new();
    let mut completed = HashSet::new();
    let mut stack = Vec::new();
    let mut cycles = Vec::new();

    fn visit(
        node: &[String],
        kind: CycleKind,
        graph: &HashMap<Vec<String>, Vec<Vec<String>>>,
        active: &mut HashSet<Vec<String>>,
        completed: &mut HashSet<Vec<String>>,
        stack: &mut Vec<Vec<String>>,
        cycles: &mut Vec<SemanticCycle>,
    ) {
        if completed.contains(node) {
            return;
        }
        active.insert(node.to_vec());
        stack.push(node.to_vec());
        for target in graph.get(node).into_iter().flatten() {
            if active.contains(target) {
                if let Some(start) = stack.iter().position(|path| path == target) {
                    let mut paths = stack[start..].to_vec();
                    paths.push(target.clone());
                    if !cycles.iter().any(|cycle| cycle.paths == paths) {
                        cycles.push(SemanticCycle { kind, paths });
                    }
                }
            } else {
                visit(target, kind, graph, active, completed, stack, cycles);
            }
        }
        stack.pop();
        active.remove(node);
        completed.insert(node.to_vec());
    }

    let nodes = graph.keys().cloned().collect::<Vec<_>>();
    for node in nodes {
        visit(
            &node,
            kind,
            &graph,
            &mut active,
            &mut completed,
            &mut stack,
            &mut cycles,
        );
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_reference_cycle() {
        let path = |name: &str| vec![name.to_owned()];
        let cycles = detect_cycles(
            CycleKind::PathReference,
            &[(path("a"), path("b")), (path("b"), path("a"))],
        );
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].kind, CycleKind::PathReference);
    }

    #[test]
    fn detects_inheritance_cycle() {
        let path = |name: &str| vec![name.to_owned()];
        let cycles = detect_cycles(
            CycleKind::Inheritance,
            &[
                (path("child"), path("parent")),
                (path("parent"), path("child")),
            ],
        );
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].kind, CycleKind::Inheritance);
    }

    #[test]
    fn detects_cycle_through_second_parent() {
        let path = |name: &str| vec![name.to_owned()];
        let cycles = detect_cycles(
            CycleKind::Inheritance,
            &[
                (path("child"), path("safe")),
                (path("child"), path("parent")),
                (path("parent"), path("child")),
            ],
        );
        assert_eq!(cycles.len(), 1);
    }
}
