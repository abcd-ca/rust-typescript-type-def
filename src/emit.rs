use crate::type_expr::{
    Array,
    DefinedTypeInfo,
    Docs,
    Ident,
    Intersection,
    NativeTypeInfo,
    Object,
    ObjectField,
    Tuple,
    TypeExpr,
    TypeInfo,
    TypeName,
    TypeString,
    Union,
};
use std::{any::TypeId, collections::HashSet, io};

/// A Rust type that has a corresponding TypeScript type definition.
///
/// For a Rust type `T`, the `TypeDef` trait defines a TypeScript type which
/// describes JavaScript value that are equivalents of Rust values of type `T`
/// as encoded to JSON using [`serde_json`](https://docs.rs/serde_json/). The
/// types are one-to-one, so decoding from TypeScript to JSON to Rust also
/// works.
///
/// You should use [`#[derive(TypeDef)]`](macro@crate::TypeDef) macro to
/// implement this trait on your own types.
///
/// This trait is implemented for basic Rust types as follows:
///
/// | Rust type | TypeScript type |
/// |---|---|
/// | [`bool`] | `boolean` |
/// | [`String`] | `string` |
/// | [`str`] | `string` |
/// | numeric types | `number`[^number] |
/// | [`()`](unit) | `null` |
/// | [`(A, B, C)`](tuple) | `[A, B, C]` |
/// | [`[T; N]`](array) | `[T, T, ..., T]` (an `N`-tuple) |
// FIXME: https://github.com/rust-lang/rust/issues/86375
/// | [`Option<T>`] | <code>T \| null</code> |
/// | [`Vec<T>`] | `T[]` |
/// | [`[T]`](slice) | `T[]` |
/// | [`HashSet<T>`](std::collections::HashSet) | `T[]` |
/// | [`BTreeSet<T>`](std::collections::BTreeSet) | `T[]` |
/// | [`HashMap<K, V>`](std::collections::HashMap) | `Record<K, V>` |
/// | [`BTreeMap<K, V>`](std::collections::BTreeMap) | `Record<K, V>` |
/// | [`&'static T`](reference) | `T` |
/// | [`Box<T>`] | `T` |
/// | [`Cow<'static, T>`](std::borrow::Cow) | `T` |
/// | [`PhantomData<T>`](std::marker::PhantomData) | `T` |
///
/// [^number]: Numeric types are emitted as named aliases converted to
/// PascalCase (e.g. `Usize`, `I32`, `F64`, `NonZeroI8`, etc.). Since they are
/// simple aliases they do not enforce anything in TypeScript about the Rust
/// types' numeric bounds, but serve to document their intended range.
pub trait TypeDef: 'static {
    /// A tuple of types which this type definition references or depends on.
    ///
    /// These type dependencies are used to discover type definitions to produce
    /// from the initial root type.
    type Deps: Deps;

    /// A constant value describing the structure of this type.
    ///
    /// This type information is used to emit a TypeScript type definition.
    const INFO: TypeInfo;
}

pub struct EmitCtx<'ctx> {
    w: &'ctx mut dyn io::Write,
    options: DefinitionFileOptions<'ctx>,
    visited: HashSet<TypeId>,
    stats: Stats,
}

/// A trait for type definition dependency lists.
///
/// This trait is sealed and only defined for tuples of types that implement
/// [`TypeDef`].
pub trait Deps: private::Sealed {
    #[doc(hidden)]
    fn emit_each(ctx: &mut EmitCtx<'_>) -> io::Result<()>;
}

pub(crate) trait Emit {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()>;
}

/// Options for customizing the output of [`write_definition_file`].
///
/// The default options are:
/// ```
/// # use typescript_type_def::DefinitionFileOptions;
/// # let default =
/// DefinitionFileOptions {
///     header: Some("AUTO-GENERATED by typescript-type-def"),
///     root_namespace: "types",
/// }
/// # ;
/// # assert_eq!(default, Default::default());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionFileOptions<'a> {
    /// The contents of a comment to be placed at the start of the file.
    ///
    /// If `Some`, the string should contain the content of the comment without
    /// any `//` and with lines separated by newline characters. If `None`,
    /// no header will be added.
    pub header: Option<&'a str>,
    /// The name of the root namespace which the definitions will be placed
    /// under.
    ///
    /// The reason all definitions must be placed under a root namespace is to
    /// prevent name ambiguities. Consider the following TypeScript module:
    /// ```typescript
    /// type Foo = number;
    /// export namespace foo {
    ///     type Foo = string;
    ///     type Bar = { x: Foo };
    /// }
    /// ```
    /// In this case, the type that `Bar.x` refers to is ambiguous; it could be
    /// either the top-level `Foo` or the adjacent `Foo`. Placing all types
    /// under a namespace and referencing them by full path removes this
    /// ambiguity:
    /// ```typescript
    /// export namespace root {
    ///     type Foo = number;
    ///     export namespace foo {
    ///         type Foo = string;
    ///         type Bar = { x: root.Foo };
    ///     }
    /// }
    /// ```
    pub root_namespace: &'a str,
}

/// Statistics about the type definitions produced by [`write_definition_file`].
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// The number of unique type definitions produced.
    pub type_definitions: usize,
}

impl<'ctx> EmitCtx<'ctx> {
    fn new(
        w: &'ctx mut dyn io::Write,
        options: DefinitionFileOptions<'ctx>,
    ) -> Self {
        Self {
            w,
            options,
            visited: Default::default(),
            stats: Default::default(),
        }
    }
}

impl Emit for TypeExpr {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        match self {
            TypeExpr::Ref(type_info) => match type_info {
                TypeInfo::Native(NativeTypeInfo { def }) => def.emit(ctx),
                TypeInfo::Defined(DefinedTypeInfo {
                    docs: _,
                    name,
                    def: _,
                }) => {
                    write!(ctx.w, "{}.", ctx.options.root_namespace)?;
                    name.emit(ctx)
                },
            },
            TypeExpr::Name(type_name) => type_name.emit(ctx),
            TypeExpr::String(type_string) => type_string.emit(ctx),
            TypeExpr::Tuple(tuple) => tuple.emit(ctx),
            TypeExpr::Object(object) => object.emit(ctx),
            TypeExpr::Array(array) => array.emit(ctx),
            TypeExpr::Union(r#union) => r#union.emit(ctx),
            TypeExpr::Intersection(intersection) => intersection.emit(ctx),
        }
    }
}

impl Emit for TypeName {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self {
            docs,
            path,
            name,
            generics,
        } = self;
        docs.emit(ctx)?;
        for path_part in *path {
            path_part.emit(ctx)?;
            write!(ctx.w, ".")?;
        }
        name.emit(ctx)?;
        if !generics.is_empty() {
            write!(ctx.w, "<")?;
            let mut first = true;
            for generic in *generics {
                if !first {
                    write!(ctx.w, ",")?;
                }
                generic.emit(ctx)?;
                first = false;
            }
            write!(ctx.w, ">")?;
        }
        Ok(())
    }
}

impl Emit for TypeString {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, value } = self;
        docs.emit(ctx)?;
        write!(ctx.w, "{:?}", value)?;
        Ok(())
    }
}

impl Emit for Tuple {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, elements } = self;
        docs.emit(ctx)?;
        write!(ctx.w, "[")?;
        let mut first = true;
        for element in *elements {
            if !first {
                write!(ctx.w, ",")?;
            }
            element.emit(ctx)?;
            first = false;
        }
        write!(ctx.w, "]")?;
        Ok(())
    }
}

impl Emit for Object {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, fields } = self;
        docs.emit(ctx)?;
        write!(ctx.w, "{{")?;
        for ObjectField {
            docs,
            name,
            optional,
            r#type,
        } in *fields
        {
            docs.emit(ctx)?;
            name.emit(ctx)?;
            if *optional {
                write!(ctx.w, "?")?;
            }
            write!(ctx.w, ":")?;
            r#type.emit(ctx)?;
            write!(ctx.w, ";")?;
        }
        write!(ctx.w, "}}")?;
        Ok(())
    }
}

impl Emit for Array {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, item } = self;
        docs.emit(ctx)?;
        write!(ctx.w, "(")?;
        item.emit(ctx)?;
        write!(ctx.w, ")[]")?;
        Ok(())
    }
}

impl Emit for Union {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, members } = self;
        docs.emit(ctx)?;
        if members.is_empty() {
            write!(ctx.w, "never")?;
        } else {
            write!(ctx.w, "(")?;
            let mut first = true;
            for part in *members {
                if !first {
                    write!(ctx.w, "|")?;
                }
                part.emit(ctx)?;
                first = false;
            }
            write!(ctx.w, ")")?;
        }
        Ok(())
    }
}

impl Emit for Intersection {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self { docs, members } = self;
        docs.emit(ctx)?;
        if members.is_empty() {
            write!(ctx.w, "any")?;
        } else {
            write!(ctx.w, "(")?;
            let mut first = true;
            for part in *members {
                if !first {
                    write!(ctx.w, "&")?;
                }
                part.emit(ctx)?;
                first = false;
            }
            write!(ctx.w, ")")?;
        }
        Ok(())
    }
}

impl Emit for Ident {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self(name) = self;
        write!(ctx.w, "{}", name)?;
        Ok(())
    }
}

impl Emit for Docs {
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        let Self(docs) = self;
        writeln!(ctx.w)?;
        writeln!(ctx.w, "/**")?;
        for line in docs.lines() {
            writeln!(ctx.w, " * {}", line)?;
        }
        writeln!(ctx.w, " */")?;
        Ok(())
    }
}

impl<T> Emit for &T
where
    T: Emit,
{
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        T::emit(self, ctx)
    }
}

impl<T> Emit for Option<T>
where
    T: Emit,
{
    fn emit(&self, ctx: &mut EmitCtx<'_>) -> io::Result<()> {
        if let Some(inner) = self {
            inner.emit(ctx)
        } else {
            Ok(())
        }
    }
}

impl EmitCtx<'_> {
    pub(crate) fn emit_type<T: ?Sized>(&mut self) -> io::Result<()>
    where
        T: TypeDef,
    {
        // TODO: can remove 'static requirement by using std::any::type_name?
        // it might not be unique though
        let type_id = TypeId::of::<T>();
        if !self.visited.contains(&type_id) {
            self.visited.insert(type_id);
            <T::Deps as Deps>::emit_each(self)?;
            self.emit_def::<T>()?;
        }
        Ok(())
    }

    fn emit_def<T: ?Sized>(&mut self) -> io::Result<()>
    where
        T: TypeDef,
    {
        match T::INFO {
            TypeInfo::Native(NativeTypeInfo { def: _ }) => Ok(()),
            TypeInfo::Defined(DefinedTypeInfo { docs, name, def }) => {
                self.stats.type_definitions += 1;
                docs.emit(self)?;
                if !name.path.is_empty() {
                    write!(self.w, "export namespace ")?;
                    let mut first = true;
                    for path_part in name.path {
                        if !first {
                            write!(self.w, ".")?;
                        }
                        path_part.emit(self)?;
                        first = false;
                    }
                    write!(self.w, "{{")?;
                }
                write!(self.w, "export type ")?;
                TypeName { path: &[], ..name }.emit(self)?;
                write!(self.w, "=")?;
                def.emit(self)?;
                write!(self.w, ";")?;
                if !name.path.is_empty() {
                    write!(self.w, "}}")?;
                }
                writeln!(self.w)?;
                Ok(())
            },
        }
    }
}

impl Default for DefinitionFileOptions<'_> {
    fn default() -> Self {
        Self {
            header: Some("AUTO-GENERATED by typescript-type-def"),
            root_namespace: "types",
        }
    }
}

/// Writes a TypeScript definition file containing type definitions for `T` to
/// the writer `W`.
///
/// The resulting TypeScript module will define and export the type definition
/// for `T` and all of its transitive dependencies under a namespace called
/// `types` (each type definition may have its own nested namespace under
/// `types` as well). The namespace `types` will also be the default export of
/// the module.
///
/// The file will also include a header comment indicating that it was
/// auto-generated by this library.
///
/// Note that the TypeScript code produced by this library is not in a
/// human-readable format. To make the code human-readable, use a TypeScript
/// code formatter, such as [Prettier](https://prettier.io/), on the file.
pub fn write_definition_file<W, T: ?Sized>(
    mut writer: W,
    options: DefinitionFileOptions<'_>,
) -> io::Result<Stats>
where
    W: io::Write,
    T: TypeDef,
{
    let mut ctx = EmitCtx::new(&mut writer, options);
    if let Some(header) = &ctx.options.header {
        for line in header.lines() {
            writeln!(ctx.w, "// {}", line)?;
        }
        writeln!(ctx.w)?;
    }
    writeln!(ctx.w, "export default {};", ctx.options.root_namespace)?;
    writeln!(ctx.w, "export namespace {}{{", ctx.options.root_namespace)?;
    ctx.emit_type::<T>()?;
    let stats = ctx.stats;
    writeln!(ctx.w, "}}")?;
    Ok(stats)
}

pub(crate) mod private {
    pub trait Sealed {}
}
