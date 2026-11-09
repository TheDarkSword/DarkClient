//! Resolution: every spec member, on every dumped version.
//!
//! For each version, a member starts from its spec form (the newest one) and
//! walks the version deltas from the newest down: a rename applies below the
//! version that introduced it, a class rename rewrites class names and
//! descriptors, a shim takes the member over. What is left is looked up in
//! that version's dump, through the class and its supertypes, which also
//! yields the class that actually declares the member.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use mc_versions::MinecraftVersion;

use crate::adapters::AdapterFile;
use crate::dump::{Dump, Member};
use crate::spec::{ClassSpec, MemberSpec, SpecFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Method,
    Field,
    Constructor,
}

/// How a member is reached on one version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Binding {
    Direct {
        /// The class that declares the member.
        class: String,
        name: String,
        descriptor: String,
        is_static: bool,
        is_final: bool,
    },
    /// A hand-written function in `mc-bindings/src/shims.rs`.
    Shim(String),
    /// The member does not exist on this version.
    Unavailable,
}

#[derive(Debug)]
pub struct ApiMember {
    pub kind: Kind,
    /// Java name as the spec writes it (`<init>` for a constructor).
    pub java: String,
    /// The descriptor the generated function follows.
    pub descriptor: String,
    pub rust: String,
    pub is_static: bool,
    pub setter: bool,
    pub nullable: bool,
    /// Element class of a collection result, as the newest version names it.
    pub element: Option<String>,
    pub params: Option<Vec<String>>,
    /// One per version, in version order.
    pub bindings: Vec<Binding>,
}

impl ApiMember {
    /// Final on every version that reaches it directly, and never shimmed.
    pub fn is_final(&self) -> bool {
        let mut direct = false;
        for binding in &self.bindings {
            match binding {
                Binding::Direct { is_final, .. } => {
                    if !is_final {
                        return false;
                    }
                    direct = true;
                }
                Binding::Shim(_) => return false,
                Binding::Unavailable => {}
            }
        }
        direct && self.kind == Kind::Field
    }
}

#[derive(Debug)]
pub enum ClassKind {
    Handle,
    /// Constants in the newest version's declaration order.
    Enum(Vec<ApiMember>),
    /// Indices into `members` of the components.
    Value(Vec<usize>),
}

#[derive(Debug)]
pub struct ApiClass {
    pub rust: String,
    /// JNI name in the newest version.
    pub name: String,
    /// One per version: the class's JNI name there, `None` when absent.
    pub names: Vec<Option<String>>,
    pub members: Vec<ApiMember>,
    pub kind: ClassKind,
    /// Nearest spec superclass, the one the Rust type derefs to.
    pub parent: Option<String>,
    pub external: bool,
}

/// Everything the code generator needs.
#[derive(Debug)]
pub struct Model {
    /// Supported versions, oldest first.
    pub versions: Vec<(String, MinecraftVersion)>,
    /// The binding group of each version: versions that resolve every class
    /// and member identically share one.
    pub groups: Vec<usize>,
    pub group_count: usize,
    pub classes: Vec<ApiClass>,
}

/// Default Rust function name: the Java name in snake case.
pub fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let out = if chars.iter().all(|c| !c.is_lowercase()) {
        name.to_lowercase()
    } else {
        let mut out = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if c.is_uppercase() {
                let previous = i.checked_sub(1).map(|p| chars[p]);
                let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                let boundary = previous.is_some_and(|p| {
                    p.is_lowercase() || p.is_ascii_digit() || (p.is_uppercase() && next_lower)
                });
                if boundary {
                    out.push('_');
                }
                out.extend(c.to_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    };
    const RESERVED: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
        "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final",
        "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    if RESERVED.contains(&out.as_str()) {
        format!("{out}_")
    } else {
        out
    }
}

/// Default Rust type name: the simple class name, nested classes joined.
fn type_name(jni: &str) -> String {
    jni.rsplit('/').next().unwrap_or(jni).replace('$', "")
}

/// Shape of a descriptor with every class erased to `L;` — what the Rust
/// signature depends on.
fn erased(descriptor: &str) -> String {
    let mut out = String::new();
    let mut chars = descriptor.chars();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == 'L' {
            for inner in chars.by_ref() {
                if inner == ';' {
                    break;
                }
            }
            out.push(';');
        }
    }
    out
}

struct Version<'a> {
    id: &'a str,
    parsed: MinecraftVersion,
    dump: &'a Dump,
}

struct Ctx<'a> {
    versions: Vec<Version<'a>>,
    /// Adapter files with the index of their version, newest first.
    deltas: Vec<(usize, &'a str, &'a AdapterFile)>,
    errors: Vec<String>,
    used_adapter_members: HashSet<(usize, String)>,
}

impl Ctx<'_> {
    fn parse_bound(&mut self, what: &str, value: &Option<String>) -> Option<MinecraftVersion> {
        let value = value.as_ref()?;
        let parsed = MinecraftVersion::parse_id(value);
        if parsed.is_none() {
            self.errors
                .push(format!("{what}: unparsable version {value:?}"));
        }
        parsed
    }

    /// A class's JNI name on version `v`, walking class renames down.
    fn class_name_at(&self, newest: &str, v: usize) -> String {
        let mut name = newest.to_string();
        for (d, _, file) in &self.deltas {
            if v < *d {
                for rename in &file.classes {
                    if rename.name == name {
                        name = rename.before.clone();
                    }
                }
            }
        }
        name
    }

    /// `descriptor` with the classes renamed above version `v` put back to
    /// their names on `v`.
    fn descriptor_at(&self, descriptor: &str, v: usize) -> String {
        let mut descriptor = descriptor.to_string();
        for (d, _, file) in &self.deltas {
            if v < *d {
                for rename in &file.classes {
                    descriptor = descriptor.replace(
                        &format!("L{};", rename.name),
                        &format!("L{};", rename.before),
                    );
                }
            }
        }
        descriptor
    }

    /// The shim providing `keys` on version `v`, if any — the newest
    /// applicable delta wins.
    fn shim_at(&mut self, keys: &[String], v: usize) -> Option<String> {
        for &(d, _, file) in &self.deltas {
            for shim in &file.shims {
                if !keys.contains(&shim.member) {
                    continue;
                }
                self.used_adapter_members.insert((d, shim.member.clone()));
                if let Some(function) = &shim.since {
                    if v >= d {
                        return Some(function.clone());
                    }
                }
                if let Some(function) = &shim.before {
                    if v < d {
                        return Some(function.clone());
                    }
                }
            }
        }
        None
    }

    /// The member's (name, descriptor) on version `v` after renames.
    fn renamed_at(
        &mut self,
        keys: &[String],
        java: &str,
        descriptor: &str,
        v: usize,
    ) -> (String, String) {
        let mut name = java.to_string();
        let mut descriptor = descriptor.to_string();
        for &(d, _, file) in &self.deltas {
            for rename in &file.renames {
                if !keys.contains(&rename.member) {
                    continue;
                }
                self.used_adapter_members.insert((d, rename.member.clone()));
                if v < d {
                    name = rename.before.clone();
                    if let Some(before) = &rename.descriptor {
                        descriptor = before.clone();
                    }
                }
            }
        }
        (name, self.descriptor_at(&descriptor, v))
    }

    /// The version id whose delta file a fix for version `v` belongs in: the
    /// next newer supported version.
    fn delta_for(&self, v: usize) -> Option<&str> {
        self.versions.get(v + 1).map(|version| version.id)
    }
}

/// Members of `kind` named `name` reachable from `class` in `dump`, with the
/// class declaring each: the class itself first, then its supertypes.
fn hierarchy<'d>(dump: &'d Dump, class: &str, kind: Kind) -> Vec<(&'d str, &'d Member)> {
    let mut out = Vec::new();
    let mut queue: VecDeque<&str> = VecDeque::from([class]);
    let mut seen = HashSet::new();
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current) {
            continue;
        }
        let Some((owner, entry)) = dump.classes.get_key_value(current) else {
            continue;
        };
        let members = match kind {
            Kind::Field => &entry.fields,
            _ => &entry.methods,
        };
        for member in members {
            let is_constructor = member.name == "<init>";
            if (kind == Kind::Constructor) == is_constructor
                && (kind != Kind::Constructor || current == class)
            {
                out.push((owner.as_str(), member));
            }
        }
        if kind != Kind::Constructor {
            queue.extend(entry.superclass.as_deref());
            queue.extend(entry.interfaces.iter().map(String::as_str));
        }
    }
    out
}

const STATIC: u16 = 0x0008;
const FINAL: u16 = 0x0010;
const ENUM: u16 = 0x4000;

#[allow(clippy::too_many_arguments)]
fn resolve_member(
    ctx: &mut Ctx,
    class_spec: &ClassSpec,
    class_rust: &str,
    class_names: &[Option<String>],
    kind: Kind,
    spec: &MemberSpec,
) -> Option<ApiMember> {
    let java = match kind {
        Kind::Constructor => "<init>".to_string(),
        _ => match &spec.name {
            Some(name) => name.clone(),
            None => {
                ctx.errors
                    .push(format!("{class_rust}: a member without a name"));
                return None;
            }
        },
    };
    let label = format!("{class_rust}.{java}");
    let since = ctx.parse_bound(&label, &spec.since);
    let until = ctx.parse_bound(&label, &spec.until);
    let available: Vec<bool> = ctx
        .versions
        .iter()
        .zip(class_names)
        .map(|(version, class)| {
            class.is_some()
                && since.is_none_or(|since| version.parsed >= since)
                && until.is_none_or(|until| version.parsed < until)
        })
        .collect();

    let mut keys = vec![label.clone()];
    if let Some(descriptor) = &spec.descriptor {
        keys.push(format!("{label}{descriptor}"));
    }

    // The descriptor the generated function follows: written in the spec, or
    // read from the newest version the member exists on directly.
    let descriptor = match (&spec.descriptor, class_spec.external) {
        (Some(descriptor), _) => descriptor.clone(),
        (None, true) => {
            ctx.errors.push(format!(
                "{label}: members of external classes need a descriptor"
            ));
            return None;
        }
        (None, false) => {
            let newest = (0..ctx.versions.len())
                .rev()
                .find(|&v| available[v] && ctx.shim_at(&keys, v).is_none());
            let Some(v) = newest else {
                ctx.errors.push(format!(
                    "{label}: no version reaches it directly; give its descriptor in the spec"
                ));
                return None;
            };
            let class = class_names[v].as_deref().unwrap_or_default();
            let found: BTreeSet<&str> = hierarchy(ctx.versions[v].dump, class, kind)
                .into_iter()
                .filter(|(_, member)| member.name == java)
                .map(|(_, member)| member.descriptor.as_str())
                .collect();
            match found.len() {
                1 => found.into_iter().next().unwrap().to_string(),
                0 => {
                    ctx.errors.push(format!(
                        "{label}: not found in {} {class}",
                        ctx.versions[v].id
                    ));
                    return None;
                }
                _ => {
                    ctx.errors.push(format!(
                        "{label}: overloaded in {}; add `descriptor` to the spec, one of: {}",
                        ctx.versions[v].id,
                        found.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                    return None;
                }
            }
        }
    };

    let mut bindings = Vec::with_capacity(ctx.versions.len());
    for v in 0..ctx.versions.len() {
        if !available[v] {
            bindings.push(Binding::Unavailable);
            continue;
        }
        if let Some(function) = ctx.shim_at(&keys, v) {
            bindings.push(Binding::Shim(function));
            continue;
        }
        let class = class_names[v].clone().unwrap_or_default();
        if class_spec.external {
            bindings.push(Binding::Direct {
                class,
                name: java.clone(),
                descriptor: descriptor.clone(),
                is_static: spec.is_static,
                is_final: false,
            });
            continue;
        }
        let (name, wanted) = ctx.renamed_at(&keys, &java, &descriptor, v);
        let version = &ctx.versions[v];
        let candidates = hierarchy(version.dump, &class, kind);
        if let Some((owner, member)) = candidates
            .iter()
            .find(|(_, member)| member.name == name && member.descriptor == wanted)
        {
            bindings.push(Binding::Direct {
                class: owner.to_string(),
                name: name.clone(),
                descriptor: wanted.clone(),
                is_static: member.flags & STATIC != 0,
                is_final: member.flags & FINAL != 0,
            });
            continue;
        }

        let same_name: Vec<String> = candidates
            .iter()
            .filter(|(_, member)| member.name == name)
            .map(|(_, member)| member.descriptor.clone())
            .collect();
        let same_shape: Vec<String> = candidates
            .iter()
            .filter(|(_, member)| {
                member.name != name && erased(&member.descriptor) == erased(&wanted)
            })
            .map(|(_, member)| format!("{}{}", member.name, member.descriptor))
            .take(8)
            .collect();
        let mut message = format!(
            "{}: {label} not found as {name}{wanted} in {class} or its supertypes",
            version.id
        );
        if !same_name.is_empty() {
            message.push_str(&format!("\n    same name: {}", same_name.join(", ")));
        }
        if !same_shape.is_empty() {
            message.push_str(&format!(
                "\n    same signature shape: {}",
                same_shape.join(", ")
            ));
        }
        match ctx.delta_for(v) {
            Some(delta) => message.push_str(&format!(
                "\n    → describe the change in mc-data/adapters/{delta}.toml (rename / shim), or set `since`"
            )),
            None => message.push_str("\n    → fix the spec (this is the newest version)"),
        }
        ctx.errors.push(message);
        bindings.push(Binding::Unavailable);
    }

    let statics: BTreeSet<bool> = bindings
        .iter()
        .filter_map(|binding| match binding {
            Binding::Direct { is_static, .. } => Some(*is_static),
            _ => None,
        })
        .collect();
    if statics.len() > 1 {
        ctx.errors
            .push(format!("{label}: static on some versions, not on others"));
    }
    let is_static = statics.into_iter().next().unwrap_or(spec.is_static);

    if spec.set && kind != Kind::Field {
        ctx.errors
            .push(format!("{label}: `set` only applies to fields"));
    }
    if (spec.nullable || spec.element.is_some()) && kind == Kind::Constructor {
        ctx.errors.push(format!(
            "{label}: a constructor is never null nor a collection"
        ));
    }
    let rust = spec.rust.clone().unwrap_or_else(|| match kind {
        Kind::Constructor => "new".to_string(),
        _ => snake_case(&java),
    });
    Some(ApiMember {
        kind,
        java,
        descriptor,
        rust,
        is_static,
        setter: spec.set,
        nullable: spec.nullable,
        element: spec.element.clone(),
        params: spec.params.clone(),
        bindings,
    })
}

fn supertypes<'d>(dump: &'d Dump, class: &'d str) -> HashSet<&'d str> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([class]);
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current) {
            continue;
        }
        if let Some(entry) = dump.classes.get(current) {
            queue.extend(entry.superclass.as_deref());
            queue.extend(entry.interfaces.iter().map(String::as_str));
        }
    }
    seen
}

pub const RESERVED_FUNCTIONS: &[&str] = &[
    "as_obj",
    "as_object",
    "cast",
    "class_name",
    "clone",
    "deref",
    "equals",
    "fmt",
    "from_object",
    "from_object_unchecked",
    "global",
    "is",
    "is_same",
    "runtime_class_name",
    "to_object",
];

/// The parent comes from the newest version but must hold on every version:
/// a child handle gets passed where the parent is expected.
fn find_parents(ctx: &mut Ctx, classes: &mut [ApiClass]) {
    let handles: Vec<usize> = (0..classes.len())
        .filter(|&i| matches!(classes[i].kind, ClassKind::Handle) && !classes[i].external)
        .collect();
    for &i in &handles {
        let Some(v) = (0..ctx.versions.len())
            .rev()
            .find(|&v| classes[i].names[v].is_some())
        else {
            continue;
        };
        let dump = ctx.versions[v].dump;
        let mut current = classes[i].names[v]
            .as_deref()
            .and_then(|name| dump.classes.get(name))
            .and_then(|entry| entry.superclass.clone());
        let mut parent = None;
        while let Some(name) = current {
            if let Some(&j) = handles
                .iter()
                .find(|&&j| classes[j].names[v].as_deref() == Some(name.as_str()))
            {
                parent = Some(j);
                break;
            }
            current = dump
                .classes
                .get(&name)
                .and_then(|entry| entry.superclass.clone());
        }
        let Some(j) = parent else { continue };
        for (u, version) in ctx.versions.iter().enumerate() {
            let (Some(child), Some(ancestor)) = (
                classes[i].names[u].as_deref(),
                classes[j].names[u].as_deref(),
            ) else {
                continue;
            };
            if !supertypes(version.dump, child).contains(ancestor) {
                ctx.errors.push(format!(
                    "{}: {child} does not extend {ancestor}, but {} derefs to {}",
                    version.id, classes[i].rust, classes[j].rust
                ));
            }
        }
        classes[i].parent = Some(classes[j].name.clone());
    }
}

/// Resolves the whole spec against the dumps. Collects every problem rather
/// than stopping at the first.
pub fn resolve(
    specs: &[(String, SpecFile)],
    dumps: &[Dump],
    adapters: &BTreeMap<String, AdapterFile>,
) -> Result<Model, Vec<String>> {
    let mut errors = Vec::new();
    let mut versions: Vec<Version> = Vec::new();
    for dump in dumps {
        match MinecraftVersion::parse_id(&dump.version) {
            Some(parsed) => versions.push(Version {
                id: &dump.version,
                parsed,
                dump,
            }),
            None => errors.push(format!("dump {:?}: unparsable version id", dump.version)),
        }
    }
    versions.sort_by_key(|version| version.parsed);
    if versions.is_empty() {
        errors.push("no dumps in mc-data/dumps".to_string());
    }

    let mut deltas = Vec::new();
    for (id, file) in adapters {
        match versions.iter().position(|version| version.id == id) {
            Some(index) => deltas.push((index, id.as_str(), file)),
            None => errors.push(format!(
                "mc-data/adapters/{id}.toml: no dump for version {id}"
            )),
        }
    }
    deltas.sort_by_key(|(index, _, _)| std::cmp::Reverse(*index));

    let mut ctx = Ctx {
        versions,
        deltas,
        errors,
        used_adapter_members: HashSet::new(),
    };

    let mut classes = Vec::new();
    let mut class_rust_names = HashMap::new();
    for (file, spec) in specs {
        for class_spec in &spec.classes {
            let rust = class_spec
                .rust
                .clone()
                .unwrap_or_else(|| type_name(&class_spec.name));
            if let Some(other) = class_rust_names.insert(rust.clone(), file.clone()) {
                ctx.errors.push(format!(
                    "{rust}: declared twice ({other}, {file}); give one a `rust` name"
                ));
            }
            let since = ctx.parse_bound(&rust, &class_spec.since);
            let mut names = Vec::new();
            for v in 0..ctx.versions.len() {
                let version = &ctx.versions[v];
                if since.is_some_and(|since| version.parsed < since) {
                    names.push(None);
                    continue;
                }
                let name = ctx.class_name_at(&class_spec.name, v);
                if !class_spec.external && !version.dump.classes.contains_key(&name) {
                    ctx.errors.push(format!(
                        "{}: class {name} ({rust}) not in the dump; add it to mc-data/classes.txt and re-dump, rename it in an adapter, or set `since`",
                        version.id
                    ));
                }
                names.push(Some(name));
            }

            let newest = (0..ctx.versions.len()).rev().find(|&v| names[v].is_some());
            let enum_constants: Option<Vec<String>> = newest.and_then(|v| {
                let entry = ctx.versions[v].dump.classes.get(names[v].as_deref()?)?;
                (!class_spec.external && entry.flags & ENUM != 0).then(|| {
                    entry
                        .fields
                        .iter()
                        .filter(|field| field.flags & (ENUM | STATIC) == ENUM | STATIC)
                        .map(|field| field.name.clone())
                        .collect()
                })
            });

            let mut members = Vec::new();
            for (kind, list) in [
                (Kind::Constructor, &class_spec.constructors),
                (Kind::Field, &class_spec.fields),
                (Kind::Method, &class_spec.methods),
            ] {
                for member in list {
                    if let Some(member) =
                        resolve_member(&mut ctx, class_spec, &rust, &names, kind, member)
                    {
                        members.push(member);
                    }
                }
            }

            let kind = if let Some(constants) = &enum_constants {
                for field in &class_spec.fields {
                    if field
                        .name
                        .as_ref()
                        .is_some_and(|name| constants.contains(name))
                    {
                        ctx.errors.push(format!(
                            "{rust}.{}: enum constants are generated as variants; remove it from the spec",
                            field.name.as_deref().unwrap_or_default()
                        ));
                    }
                }
                if class_spec.value.is_some() {
                    ctx.errors
                        .push(format!("{rust}: an enum cannot be a value class"));
                }
                let mut resolved = Vec::new();
                for constant in constants {
                    let spec = MemberSpec {
                        name: Some(constant.clone()),
                        ..MemberSpec::default()
                    };
                    if let Some(member) =
                        resolve_member(&mut ctx, class_spec, &rust, &names, Kind::Field, &spec)
                    {
                        resolved.push(member);
                    }
                }
                ClassKind::Enum(resolved)
            } else if let Some(components) = &class_spec.value {
                let mut indices = Vec::new();
                for component in components {
                    match members.iter().position(|member: &ApiMember| {
                        member.java == *component
                            && !member.is_static
                            && (member.kind == Kind::Field
                                || (member.kind == Kind::Method
                                    && member.descriptor.starts_with("()")))
                    }) {
                        Some(index) => indices.push(index),
                        None => ctx.errors.push(format!(
                            "{rust}: value component {component} is not a declared field or no-argument method"
                        )),
                    }
                }
                for (index, member) in members.iter().enumerate() {
                    if !member.is_static
                        && member.kind != Kind::Constructor
                        && !indices.contains(&index)
                    {
                        ctx.errors.push(format!(
                            "{rust}.{}: a value class only has its components, constructors and static members",
                            member.java
                        ));
                    }
                }
                ClassKind::Value(indices)
            } else {
                ClassKind::Handle
            };

            let mut seen = HashSet::new();
            for member in &members {
                let mut functions = vec![member.rust.clone()];
                if member.setter {
                    functions.push(format!("set_{}", member.rust));
                }
                for function in functions {
                    if RESERVED_FUNCTIONS.contains(&function.as_str())
                        || !seen.insert(function.clone())
                    {
                        ctx.errors.push(format!(
                            "{rust}::{function} is generated twice; give a member a `rust` name"
                        ));
                    }
                }
            }

            classes.push(ApiClass {
                rust,
                name: class_spec.name.clone(),
                names,
                members,
                kind,
                parent: None,
                external: class_spec.external,
            });
        }
    }

    find_parents(&mut ctx, &mut classes);

    // Adapter entries that matched no spec member are typos.
    for &(d, id, file) in &ctx.deltas {
        let members = file
            .renames
            .iter()
            .map(|rename| &rename.member)
            .chain(file.shims.iter().map(|shim| &shim.member));
        for member in members {
            if !ctx.used_adapter_members.contains(&(d, member.clone())) {
                ctx.errors.push(format!(
                    "mc-data/adapters/{id}.toml: {member} matches no spec member"
                ));
            }
        }
    }

    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }

    // Versions that resolve every class and member identically share a group.
    let mut groups = Vec::new();
    let mut signatures: Vec<Vec<String>> = Vec::new();
    for v in 0..ctx.versions.len() {
        let signature: Vec<String> = classes
            .iter()
            .flat_map(|class| {
                let constants: &[ApiMember] = match &class.kind {
                    ClassKind::Enum(constants) => constants,
                    _ => &[],
                };
                std::iter::once(format!("{:?}", class.names[v])).chain(
                    class
                        .members
                        .iter()
                        .chain(constants)
                        .map(move |member| format!("{:?}", member.bindings[v])),
                )
            })
            .collect();
        let group = match signatures.iter().position(|other| *other == signature) {
            Some(group) => group,
            None => {
                signatures.push(signature);
                signatures.len() - 1
            }
        };
        groups.push(group);
    }

    Ok(Model {
        versions: ctx
            .versions
            .iter()
            .map(|version| (version.id.to_string(), version.parsed))
            .collect(),
        group_count: signatures.len(),
        groups,
        classes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::{Class, Member};

    /// `(class, superclass, [(method, descriptor, flags)])`.
    type ClassRow<'a> = (&'a str, Option<&'a str>, &'a [(&'a str, &'a str, u16)]);

    fn dump(version: &str, classes: &[ClassRow]) -> Dump {
        let mut dump = Dump {
            version: version.into(),
            ..Dump::default()
        };
        for (name, superclass, methods) in classes {
            let mut class = Class {
                superclass: superclass.map(String::from),
                ..Class::default()
            };
            for (method, descriptor, flags) in methods.iter() {
                class.methods.insert(Member {
                    name: method.to_string(),
                    descriptor: descriptor.to_string(),
                    flags: *flags,
                });
            }
            dump.classes.insert(name.to_string(), class);
        }
        dump
    }

    fn spec(text: &str) -> Vec<(String, SpecFile)> {
        vec![("spec.toml".into(), toml::from_str(text).unwrap())]
    }

    fn adapters(entries: &[(&str, &str)]) -> BTreeMap<String, AdapterFile> {
        entries
            .iter()
            .map(|(id, text)| (id.to_string(), toml::from_str(text).unwrap()))
            .collect()
    }

    #[test]
    fn snake_case_follows_rust_conventions() {
        assert_eq!(snake_case("getBbWidth"), "get_bb_width");
        assert_eq!(snake_case("xOld"), "x_old");
        assert_eq!(snake_case("yRotO"), "y_rot_o");
        assert_eq!(snake_case("TOTEM_OF_UNDYING"), "totem_of_undying");
        assert_eq!(snake_case("getUUID"), "get_uuid");
        assert_eq!(snake_case("type"), "type_");
    }

    #[test]
    fn a_rename_applies_below_the_version_that_made_it() {
        let dumps = [
            dump(
                "26.1",
                &[("a/Renderer", None, &[("getMainCamera", "()La/Camera;", 1)])],
            ),
            dump(
                "26.2",
                &[("a/Renderer", None, &[("mainCamera", "()La/Camera;", 1)])],
            ),
        ];
        let model = resolve(
            &spec("[[class]]\nname = \"a/Renderer\"\n[[class.method]]\nname = \"mainCamera\"\n"),
            &dumps,
            &adapters(&[(
                "26.2",
                "[[rename]]\nmember = \"Renderer.mainCamera\"\nbefore = \"getMainCamera\"\n",
            )]),
        )
        .unwrap();
        let member = &model.classes[0].members[0];
        assert_eq!(member.rust, "main_camera");
        assert!(
            matches!(&member.bindings[0], Binding::Direct { name, .. } if name == "getMainCamera")
        );
        assert!(
            matches!(&member.bindings[1], Binding::Direct { name, .. } if name == "mainCamera")
        );
        assert_eq!(model.group_count, 2);
    }

    #[test]
    fn inherited_members_resolve_to_their_declaring_class() {
        let dumps = [dump(
            "26.1",
            &[
                ("a/Player", Some("a/Entity"), &[]),
                ("a/Entity", None, &[("getId", "()I", 1)]),
            ],
        )];
        let model = resolve(
            &spec("[[class]]\nname = \"a/Player\"\n[[class.method]]\nname = \"getId\"\n"),
            &dumps,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(matches!(
            &model.classes[0].members[0].bindings[0],
            Binding::Direct { class, .. } if class == "a/Entity"
        ));
    }

    #[test]
    fn class_renames_rewrite_names_and_descriptors() {
        let dumps = [
            dump(
                "26.2",
                &[("old/Surface", None, &[("device", "()Lold/Device;", 1)])],
            ),
            dump(
                "26.3",
                &[("new/Surface", None, &[("device", "()Lnew/Device;", 1)])],
            ),
        ];
        let model = resolve(
            &spec("[[class]]\nname = \"new/Surface\"\n[[class.method]]\nname = \"device\"\n"),
            &dumps,
            &adapters(&[(
                "26.3",
                "[[class]]\nname = \"new/Surface\"\nbefore = \"old/Surface\"\n[[class]]\nname = \"new/Device\"\nbefore = \"old/Device\"\n",
            )]),
        )
        .unwrap();
        let class = &model.classes[0];
        assert_eq!(class.names[0].as_deref(), Some("old/Surface"));
        assert!(matches!(
            &class.members[0].bindings[0],
            Binding::Direct { descriptor, .. } if descriptor == "()Lold/Device;"
        ));
    }

    #[test]
    fn a_since_shim_takes_over_from_its_version_on() {
        let dumps = [
            dump("26.2", &[("a/Mob", None, &[("swing", "(La/Hand;)V", 1)])]),
            dump(
                "26.3",
                &[("a/Mob", None, &[("swing", "(La/Hand;La/Anim;Z)Z", 1)])],
            ),
        ];
        let model = resolve(
            &spec("[[class]]\nname = \"a/Mob\"\n[[class.method]]\nname = \"swing\"\ndescriptor = \"(La/Hand;)V\"\n"),
            &dumps,
            &adapters(&[("26.3", "[[shim]]\nmember = \"Mob.swing\"\nsince = \"swing_with_animation\"\n")]),
        )
        .unwrap();
        let member = &model.classes[0].members[0];
        assert!(matches!(member.bindings[0], Binding::Direct { .. }));
        assert_eq!(
            member.bindings[1],
            Binding::Shim("swing_with_animation".into())
        );
    }

    #[test]
    fn a_missing_member_reports_candidates_and_where_to_fix_it() {
        let dumps = [
            dump(
                "26.1",
                &[("a/Renderer", None, &[("getMainCamera", "()La/Camera;", 1)])],
            ),
            dump(
                "26.2",
                &[("a/Renderer", None, &[("mainCamera", "()La/Camera;", 1)])],
            ),
        ];
        let errors = resolve(
            &spec("[[class]]\nname = \"a/Renderer\"\n[[class.method]]\nname = \"mainCamera\"\n"),
            &dumps,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("26.1: Renderer.mainCamera not found"));
        assert!(errors[0].contains("getMainCamera()La/Camera;"));
        assert!(errors[0].contains("mc-data/adapters/26.2.toml"));
    }

    #[test]
    fn an_overloaded_name_needs_a_descriptor() {
        let dumps = [dump(
            "26.1",
            &[(
                "a/E",
                None,
                &[("dist", "(DDD)D", 1), ("dist", "(La/V;)D", 1)],
            )],
        )];
        let errors = resolve(
            &spec("[[class]]\nname = \"a/E\"\n[[class.method]]\nname = \"dist\"\n"),
            &dumps,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(errors[0].contains("overloaded"));
    }

    fn member(name: &str, descriptor: &str, flags: u16) -> Member {
        Member {
            name: name.into(),
            descriptor: descriptor.into(),
            flags,
        }
    }

    #[test]
    fn a_java_enum_becomes_a_rust_enum_with_its_constants() {
        let mut dumps = [dump("26.1", &[("a/Hand", None, &[])])];
        let hand = dumps[0].classes.get_mut("a/Hand").unwrap();
        hand.flags = ENUM;
        hand.fields
            .insert(member("MAIN", "La/Hand;", ENUM | STATIC | FINAL));
        hand.fields
            .insert(member("OFF", "La/Hand;", ENUM | STATIC | FINAL));
        hand.fields
            .insert(member("BY_ID", "Ljava/util/Map;", STATIC | FINAL));
        let model = resolve(
            &spec("[[class]]\nname = \"a/Hand\"\n"),
            &dumps,
            &BTreeMap::new(),
        )
        .unwrap();
        let ClassKind::Enum(constants) = &model.classes[0].kind else {
            panic!("not an enum");
        };
        let names: Vec<&str> = constants.iter().map(|c| c.java.as_str()).collect();
        assert_eq!(names, ["MAIN", "OFF"]);
    }

    #[test]
    fn a_handle_derefs_to_its_nearest_spec_superclass_on_every_version() {
        let dumps = [
            dump("26.1", &[("a/Local", None, &[]), ("a/Player", None, &[])]),
            dump(
                "26.2",
                &[
                    ("a/Local", Some("a/Client"), &[]),
                    ("a/Client", Some("a/Player"), &[]),
                    ("a/Player", None, &[]),
                ],
            ),
        ];
        let text = "[[class]]\nname = \"a/Local\"\n[[class]]\nname = \"a/Player\"\n";
        let errors = resolve(&spec(text), &dumps, &BTreeMap::new()).unwrap_err();
        assert!(errors[0].contains("26.1: a/Local does not extend a/Player"));

        let model = resolve(&spec(text), &dumps[1..], &BTreeMap::new()).unwrap();
        assert_eq!(model.classes[0].parent.as_deref(), Some("a/Player"));
        assert_eq!(model.classes[1].parent, None);
    }

    #[test]
    fn a_value_class_has_only_components_and_constructors() {
        let mut dumps = [dump(
            "26.1",
            &[(
                "a/Pos",
                None,
                &[("<init>", "(II)V", 1), ("up", "()La/Pos;", 1)],
            )],
        )];
        let pos = dumps[0].classes.get_mut("a/Pos").unwrap();
        pos.fields.insert(member("x", "I", FINAL));
        pos.fields.insert(member("y", "I", FINAL));
        let ok = "[[class]]\nname = \"a/Pos\"\nvalue = [\"x\", \"y\"]\n[[class.constructor]]\ndescriptor = \"(II)V\"\n[[class.field]]\nname = \"x\"\n[[class.field]]\nname = \"y\"\n";
        let model = resolve(&spec(ok), &dumps, &BTreeMap::new()).unwrap();
        assert!(
            matches!(&model.classes[0].kind, ClassKind::Value(components) if components.len() == 2)
        );

        let extra = format!("{ok}[[class.method]]\nname = \"up\"\n");
        let errors = resolve(&spec(&extra), &dumps, &BTreeMap::new()).unwrap_err();
        assert!(errors[0].contains("a value class only has its components"));
    }
}
