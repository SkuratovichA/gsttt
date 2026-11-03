//! What the whole file says about its types, which the reading of one declaration needs before it starts.

use std::collections::{HashMap, HashSet};

use gsttt_core::fact::{Import, MEMBER_SEPARATOR};
use ra_ap_syntax::ast::{self, HasGenericArgs, HasGenericParams, HasName, HasTypeBounds};
use ra_ap_syntax::{AstNode, SyntaxNode};

use crate::imports::binding;
use crate::syntax::text;

/// The types the prelude brings in, whose methods are the standard library's.
const PRELUDE: &[&str] = &[
    "String", "Vec", "Option", "Result", "Box", "Some", "None", "Ok", "Err",
];

/// The traits that say something about a value and nothing about what it does.
const MARKERS: &[&str] = &["Send", "Sync", "Sized", "Unpin", "Copy"];

/// Whether `import` takes from a crate of the standard library.
pub(super) fn of_std(import: &Import) -> bool {
    ["std::", "core::", "alloc::"]
        .iter()
        .any(|root| import.spec.starts_with(root))
}

/// The items of a file, with the inline modules flattened into it.
pub(super) fn flatten(node: &SyntaxNode, out: &mut Vec<ast::Item>) {
    for item in node.children().filter_map(ast::Item::cast) {
        match item {
            ast::Item::Module(module) => {
                if let Some(list) = module.item_list() {
                    flatten(list.syntax(), out);
                }
            }
            other => out.push(other),
        }
    }
}

/// The name a type is called by: the last segment of its path, through references; the trait of a `dyn` or an `impl` type.
pub(super) fn last_segment(ty: &ast::Type) -> Option<String> {
    match ty {
        ast::Type::RefType(ty) => last_segment(&ty.ty()?),
        ast::Type::ParenType(ty) => last_segment(&ty.ty()?),
        ast::Type::PathType(ty) => ty.path()?.segment()?.name_ref().map(|name| text(&name)),
        ast::Type::DynTraitType(ty) => bound_head(&ty.type_bound_list()?),
        ast::Type::ImplTraitType(ty) => bound_head(&ty.type_bound_list()?),
        _ => None,
    }
}

/// The first trait among `bounds` that is more than a marker.
fn bound_head(bounds: &ast::TypeBoundList) -> Option<String> {
    bounds
        .bounds()
        .filter_map(|bound| bound.ty())
        .filter_map(|ty| last_segment(&ty))
        .find(|name| !MARKERS.contains(&name.as_str()))
}

/// The type an `impl` block is for, when it names one: `Foo` for `impl Foo`, `impl Tr for &Foo` and `impl<T> Tr for Foo<T>`. A type parameter of the block is no type to name.
pub(super) fn impl_target(item: &ast::Impl, generics: &Generics) -> Option<String> {
    let head = last_segment(&item.self_ty()?)?;
    (!generics.contains(&head)).then_some(head)
}

/// Every name the file declares a member for, the types of its fields and its type aliases.
pub(crate) struct Table {
    /// `Type.member` for each method, constant, associated type, variant and field the file declares.
    members: HashSet<String>,
    /// The type of each named field, where it is a type of a class.
    fields: HashMap<(String, String), String>,
    /// `Class.member` for each field and each method taking `self` whose written type is a class, with that class; `Option<Foo>` and `Result<Foo, E>` hold `Foo`. It is apart from `fields`, which a method is not in and which keeps `Option` as the class of a field.
    holds: HashSet<(String, String)>,
    /// Type aliases, which stand for a type this file does not read through.
    aliases: HashSet<String>,
    /// Names bound from the standard library, whose methods are not any file's.
    external: HashSet<String>,
}

impl Table {
    pub(super) fn of(items: &[ast::Item], imports: &[Import]) -> Self {
        let mut table = Self {
            members: HashSet::new(),
            fields: HashMap::new(),
            holds: HashSet::new(),
            aliases: HashSet::new(),
            external: imports
                .iter()
                .filter(|import| of_std(import))
                .filter_map(binding)
                .map(str::to_owned)
                .collect(),
        };
        for item in items {
            if let ast::Item::TypeAlias(alias) = item
                && let Some(name) = alias.name()
            {
                table.aliases.insert(text(&name));
            }
        }
        for item in items {
            table.add(item);
        }
        table
    }

    fn member(&mut self, class: &str, member: &str) {
        self.members
            .insert(format!("{class}{MEMBER_SEPARATOR}{member}"));
    }

    fn add(&mut self, item: &ast::Item) {
        match item {
            ast::Item::Struct(item) => {
                let Some(name) = item.name().map(|name| text(&name)) else {
                    return;
                };
                if let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() {
                    self.record(&name, &fields, &Generics::of(item));
                }
            }
            ast::Item::Union(item) => {
                if let (Some(name), Some(fields)) = (item.name(), item.record_field_list()) {
                    self.record(&text(&name), &fields, &Generics::of(item));
                }
            }
            ast::Item::Enum(item) => {
                if let Some(name) = item.name().map(|name| text(&name)) {
                    for variant in item
                        .variant_list()
                        .iter()
                        .flat_map(ast::VariantList::variants)
                    {
                        if let Some(variant) = variant.name() {
                            self.member(&name, &text(&variant));
                        }
                    }
                }
            }
            ast::Item::Trait(item) => {
                if let Some(name) = item.name().map(|name| text(&name)) {
                    let generics = Generics::of(item);
                    for member in item
                        .assoc_item_list()
                        .iter()
                        .flat_map(ast::AssocItemList::assoc_items)
                    {
                        if let Some(label) = super::assoc_name(&member) {
                            self.member(&name, &label);
                            self.hold_returned(&name, &label, &member, &generics);
                        }
                    }
                }
            }
            ast::Item::Impl(item) => {
                let generics = Generics::of(item);
                let head = impl_target(item, &generics)
                    .or_else(|| item.trait_().and_then(|ty| last_segment(&ty)));
                if let Some(head) = head {
                    for member in item
                        .assoc_item_list()
                        .iter()
                        .flat_map(ast::AssocItemList::assoc_items)
                    {
                        if let Some(label) = super::assoc_name(&member) {
                            self.member(&head, &label);
                            self.hold_returned(&head, &label, &member, &generics);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn record(&mut self, name: &str, fields: &ast::RecordFieldList, generics: &Generics) {
        for field in fields.fields() {
            let Some(field_name) = field.name().map(|name| text(&name)) else {
                continue;
            };
            self.member(name, &field_name);
            let Some(ty) = field.ty() else {
                continue;
            };
            self.hold(name, &field_name, &ty, generics);
            if let Some(class) = self.class_of(&ty, generics) {
                self.fields.insert((name.to_owned(), field_name), class);
            }
        }
    }

    /// Records that `owner.member` holds the class of `ty`, unless that is `Self`, the owner itself or a class of the standard library, which no file reads a chain through.
    fn hold(&mut self, owner: &str, member: &str, ty: &ast::Type, generics: &Generics) {
        let Some(class) = self.held_class(ty, generics) else {
            return;
        };
        if class != "Self" && class != owner && !self.is_std(&class) {
            self.holds
                .insert((super::member_name(owner, member), class));
        }
    }

    /// Records what a method that takes `self` returns, as a holder. An associated function without `self` is called on the class and reads through no value.
    fn hold_returned(
        &mut self,
        owner: &str,
        member: &str,
        item: &ast::AssocItem,
        generics: &Generics,
    ) {
        let ast::AssocItem::Fn(method) = item else {
            return;
        };
        let takes_self = method
            .param_list()
            .is_some_and(|list| list.self_param().is_some());
        if let Some(ty) = method.ret_type().and_then(|ret| ret.ty())
            && takes_self
        {
            self.hold(owner, member, &ty, &generics.with(method));
        }
    }

    /// The class a member that is written as `ty` hands out: as `class_of` says it, looking inside `Option` and `Result` for the value, because `x.get().expect("..").read()` reads through the `Foo` of an `Option<Foo>`.
    fn held_class(&self, ty: &ast::Type, generics: &Generics) -> Option<String> {
        match ty {
            ast::Type::RefType(ty) => self.held_class(&ty.ty()?, generics),
            ast::Type::ParenType(ty) => self.held_class(&ty.ty()?, generics),
            ast::Type::PathType(path) => {
                let segment = path.path()?.segment()?;
                if matches!(text(&segment.name_ref()?).as_str(), "Option" | "Result") {
                    return self.held_class(&first_type_arg(&segment)?, generics);
                }
                // `Self::Item` is a type only the implementation knows.
                let first = path.path()?.first_segment()?.name_ref()?;
                if path.path()?.qualifier().is_some() && text(&first) == "Self" {
                    return None;
                }
                self.class_of(ty, generics)
            }
            _ => self.class_of(ty, generics),
        }
    }

    /// Each member of a class of the file that holds a value of a class, as the name `Class.member` and the class it holds. The order is not defined.
    pub(super) fn holders(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.holds
            .iter()
            .map(|(name, class)| (name.as_str(), class.as_str()))
    }

    /// Whether the file declares `class.member`.
    pub(super) fn declares(&self, class: &str, member: &str) -> bool {
        self.members
            .contains(&format!("{class}{MEMBER_SEPARATOR}{member}"))
    }

    /// The type of the field `field` of `class`, when the file writes it as a type of a class.
    pub(super) fn field(&self, class: &str, field: &str) -> Option<&String> {
        self.fields.get(&(class.to_owned(), field.to_owned()))
    }

    /// Whether the methods of `class` are the standard library's.
    pub(super) fn is_std(&self, class: &str) -> bool {
        PRELUDE.contains(&class) || self.external.contains(class)
    }

    /// The class a type is a type of: its name, when it is a name of CamelCase that is no parameter of the code around it and no alias. A `dyn Tr` and an `impl Tr` are of the class `Tr`.
    pub(super) fn class_of(&self, ty: &ast::Type, generics: &Generics) -> Option<String> {
        let ty = unwrapped(ty);
        // `T::Item` is a type only `T` knows.
        if let ast::Type::PathType(path) = &ty
            && let Some(first) = path.path().and_then(|path| path.first_segment())
            && let Some(first) = first.name_ref()
            && path.path().is_some_and(|path| path.qualifier().is_some())
            && generics.contains(&text(&first))
        {
            return None;
        }
        let name = last_segment(&ty)?;
        if let Some(bound) = generics.bound(&name) {
            return Some(bound.to_owned());
        }
        let is_class = name.starts_with(char::is_uppercase)
            && !generics.contains(&name)
            && !self.aliases.contains(&name);
        is_class.then_some(name)
    }
}

/// The pointers and lazy cells whose methods are those of what they hold: `Arc<Foo>` and `Lazy<Foo>` are of the class `Foo`.
const HOLDERS: &[&str] = &["Box", "Rc", "Arc", "Lazy", "LazyLock"];

/// The first type among the generic arguments of a path segment.
fn first_type_arg(segment: &ast::PathSegment) -> Option<ast::Type> {
    segment
        .generic_arg_list()?
        .generic_args()
        .find_map(|arg| match arg {
            ast::GenericArg::TypeArg(arg) => arg.ty(),
            _ => None,
        })
}

/// The type inside the holders that wrap `ty`, `ty` itself when it is held by none.
fn unwrapped(ty: &ast::Type) -> ast::Type {
    let held = match ty {
        ast::Type::PathType(path) => path
            .path()
            .and_then(|path| path.segment())
            .filter(|segment| {
                segment
                    .name_ref()
                    .is_some_and(|name| HOLDERS.contains(&text(&name).as_str()))
            })
            .and_then(|segment| first_type_arg(&segment)),
        _ => None,
    };
    held.map_or_else(|| ty.clone(), |inner| unwrapped(&inner))
}

/// The type parameters of the code around a type.
#[derive(Clone, Default)]
pub(super) struct Generics {
    names: HashSet<String>,
    /// The first trait that is more than a marker each parameter is bound by, so that a value of type `T` with `T: Tr` is read as one of `impl Tr`.
    bounds: HashMap<String, String>,
}

impl Generics {
    /// The parameters an item declares, with their bounds and those of its `where`.
    pub(super) fn of(item: &impl HasGenericParams) -> Self {
        Self::default().with(item)
    }

    /// These parameters and those an item declares, as a method has the parameters of its block and its own.
    pub(super) fn with(&self, item: &impl HasGenericParams) -> Self {
        let mut generics = self.clone();
        for param in item
            .generic_param_list()
            .into_iter()
            .flat_map(|list| list.generic_params())
        {
            match param {
                ast::GenericParam::TypeParam(param) => generics.add_type(&param),
                ast::GenericParam::ConstParam(param) => generics.add_const(&param),
                ast::GenericParam::LifetimeParam(_) => {}
            }
        }
        for pred in item
            .where_clause()
            .into_iter()
            .flat_map(|clause| clause.predicates())
        {
            generics.add_where(&pred);
        }
        generics
    }

    pub(super) fn add_type(&mut self, param: &ast::TypeParam) {
        let Some(name) = param.name().map(|name| text(&name)) else {
            return;
        };
        if let Some(head) = param.type_bound_list().and_then(|list| bound_head(&list)) {
            self.bounds.insert(name.clone(), head);
        }
        self.names.insert(name);
    }

    pub(super) fn add_const(&mut self, param: &ast::ConstParam) {
        if let Some(name) = param.name() {
            self.names.insert(text(&name));
        }
    }

    /// `where T: Tr` on a parameter already declared. `where T::Output: Tr` says nothing about `T`, and a type of the file in a `where` is no parameter.
    pub(super) fn add_where(&mut self, pred: &ast::WherePred) {
        let Some(ast::Type::PathType(ty)) = pred.ty() else {
            return;
        };
        let Some(path) = ty.path().filter(|path| path.qualifier().is_none()) else {
            return;
        };
        let Some(name) = path.segment().and_then(|segment| segment.name_ref()) else {
            return;
        };
        let name = text(&name);
        if self.names.contains(&name)
            && let Some(head) = pred.type_bound_list().and_then(|list| bound_head(&list))
        {
            self.bounds.entry(name).or_insert(head);
        }
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub(super) fn bound(&self, name: &str) -> Option<&str> {
        self.bounds.get(name).map(String::as_str)
    }
}
