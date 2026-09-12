use naga::{
    AddressSpace, Expression, Function, GlobalVariable, Handle, MemoryDecorations, ResourceBinding,
    Span, StorageAccess, Type,
};
use syn::{Attribute, ForeignItem, ItemForeignMod, ItemStatic, LitInt};

use super::env::{Env, Slot};
use super::Context;
use crate::Error;

pub(crate) struct GlobalInfo {
    pub name: String,
    pub handle: Handle<GlobalVariable>,
    pub ty: Handle<Type>,
    pub writable: bool,
}

#[derive(Clone, Copy)]
enum SpaceKind {
    Uniform,
    Storage {
        write: bool,
    },
    /// Shared across a workgroup, zero-initialised each dispatch.
    Workgroup,
    /// Private to each invocation.
    Private,
}

struct ResourceInfo {
    group: Option<u32>,
    binding: Option<u32>,
    space: Option<SpaceKind>,
}

pub(super) fn bind_globals(ctx: &Context, function: &mut Function, env: &mut Env) {
    for g in &ctx.globals {
        let expr = function
            .expressions
            .append(Expression::GlobalVariable(g.handle), Span::UNDEFINED);
        // A handle names the resource itself; there is nothing to load from it,
        // and Naga wants the `GlobalVariable` expression passed straight to the
        // image builtins.
        let slot = if super::texture::is_handle(ctx, g.ty) {
            Slot::Value(expr)
        } else {
            Slot::Ptr(expr)
        };
        env.push_rw(g.name.clone(), slot, g.ty, g.writable);
    }
}

pub(super) fn lower_static(ctx: &mut Context, item: ItemStatic) -> Result<(), Error> {
    let name = item.ident.to_string();
    let ty = ctx.lower_type(&item.ty)?;
    insert_global(ctx, name, ty, &item.attrs)
}

pub(super) fn lower_foreign_mod(ctx: &mut Context, item: ItemForeignMod) -> Result<(), Error> {
    for foreign in item.items {
        match foreign {
            ForeignItem::Static(st) => {
                let name = st.ident.to_string();
                let ty = ctx.lower_type(&st.ty)?;
                insert_global(ctx, name, ty, &st.attrs)?;
            }
            other => {
                return Err(Error::UnsupportedItem(foreign_kind(&other)));
            }
        }
    }
    Ok(())
}

fn insert_global(
    ctx: &mut Context,
    name: String,
    ty: Handle<Type>,
    attrs: &[Attribute],
) -> Result<(), Error> {
    let info = parse_resource_attrs(attrs)?;
    // Both or neither: a host that assigns bindings itself (Blade matches
    // globals up by name at pipeline creation) wants them left unset, but half
    // a binding is a typo.
    let binding = match (info.group, info.binding) {
        (Some(group), Some(binding)) => Some(ResourceBinding { group, binding }),
        (None, None) => None,
        _ => return Err(Error::MissingResourceBinding(name.clone())),
    };

    if ctx.globals.iter().any(|g| g.name == name) {
        return Err(Error::DuplicateGlobal(name));
    }

    // A texture or sampler is a handle, not a buffer: it has no address space
    // to choose and is never written through an assignment.
    if super::texture::is_handle(ctx, ty) {
        if info.space.is_some() {
            return Err(Error::UnexpectedAddressSpace(name));
        }
        return finish_global(ctx, name, ty, AddressSpace::Handle, false, binding);
    }

    let (space, writable) = match info.space.unwrap_or(SpaceKind::Uniform) {
        SpaceKind::Uniform => (AddressSpace::Uniform, false),
        SpaceKind::Storage { write } => {
            let access = if write {
                StorageAccess::LOAD | StorageAccess::STORE
            } else {
                StorageAccess::LOAD
            };
            (AddressSpace::Storage { access }, write)
        }
        SpaceKind::Workgroup => (AddressSpace::WorkGroup, true),
        SpaceKind::Private => (AddressSpace::Private, true),
    };

    // Only resources are bound; workgroup and private memory belongs to the
    // shader itself.
    let binding = match (space, binding) {
        (AddressSpace::WorkGroup | AddressSpace::Private, Some(_)) => {
            return Err(Error::UnexpectedBinding(name))
        }
        (_, binding) => binding,
    };

    // WGSL puts runtime-sized arrays in storage only. Naga notices too, but as
    // an alignment complaint about a stride nobody wrote.
    if !matches!(space, AddressSpace::Storage { .. }) && has_runtime_array(ctx, ty) {
        return Err(Error::RuntimeArrayNotStorage(name));
    }

    finish_global(ctx, name, ty, space, writable, binding)
}

fn finish_global(
    ctx: &mut Context,
    name: String,
    ty: Handle<Type>,
    space: AddressSpace,
    writable: bool,
    binding: Option<ResourceBinding>,
) -> Result<(), Error> {
    let handle = ctx.module.global_variables.append(
        GlobalVariable {
            name: Some(name.clone()),
            space,
            binding,
            ty,
            init: None,
            memory_decorations: MemoryDecorations::empty(),
        },
        Span::UNDEFINED,
    );
    ctx.globals.push(GlobalInfo {
        name,
        handle,
        ty,
        writable,
    });
    Ok(())
}

/// Is `ty` a runtime-sized array, or a struct ending in one?
fn has_runtime_array(ctx: &Context, ty: Handle<Type>) -> bool {
    if matches!(ctx.as_array(ty), Some((_, naga::ArraySize::Dynamic))) {
        return true;
    }
    match ctx.as_struct(ty).and_then(|members| members.last()) {
        Some(last) => has_runtime_array(ctx, last.ty),
        None => false,
    }
}

fn parse_resource_attrs(attrs: &[Attribute]) -> Result<ResourceInfo, Error> {
    let mut info = ResourceInfo {
        group: None,
        binding: None,
        space: None,
    };
    for attr in attrs {
        if attr.path().is_ident("group") {
            if info.group.is_some() {
                return Err(Error::DuplicateAttribute("group".into()));
            }
            info.group = Some(parse_u32_arg(attr, "group")?);
        } else if attr.path().is_ident("binding") {
            if info.binding.is_some() {
                return Err(Error::DuplicateAttribute("binding".into()));
            }
            info.binding = Some(parse_u32_arg(attr, "binding")?);
        } else if attr.path().is_ident("uniform") {
            set_space(&mut info.space, SpaceKind::Uniform)?;
        } else if attr.path().is_ident("storage") {
            let write = parse_storage_write(attr)?;
            set_space(&mut info.space, SpaceKind::Storage { write })?;
        } else if attr.path().is_ident("workgroup") {
            set_space(&mut info.space, SpaceKind::Workgroup)?;
        } else if attr.path().is_ident("private") {
            set_space(&mut info.space, SpaceKind::Private)?;
        }
    }
    Ok(info)
}

fn set_space(slot: &mut Option<SpaceKind>, space: SpaceKind) -> Result<(), Error> {
    if slot.is_some() {
        return Err(Error::DuplicateAttribute("address space".into()));
    }
    *slot = Some(space);
    Ok(())
}

fn parse_u32_arg(attr: &Attribute, what: &str) -> Result<u32, Error> {
    let lit: LitInt = attr
        .parse_args()
        .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
    lit.base10_parse()
        .map_err(|_| Error::UnsupportedBinding(what.into()))
}

fn parse_storage_write(attr: &Attribute) -> Result<bool, Error> {
    if attr.meta.require_path_only().is_ok() {
        return Ok(false);
    }
    let ident: syn::Ident = attr
        .parse_args()
        .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
    match ident.to_string().as_str() {
        "read" => Ok(false),
        "read_write" | "write" => Ok(true),
        other => Err(Error::UnsupportedBinding(other.into())),
    }
}

fn foreign_kind(item: &ForeignItem) -> String {
    match item {
        ForeignItem::Fn(_) => "extern fn",
        ForeignItem::Static(_) => "extern static",
        ForeignItem::Type(_) => "extern type",
        ForeignItem::Macro(_) => "extern macro",
        _ => "extern item",
    }
    .into()
}
