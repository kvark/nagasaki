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
    Storage { write: bool },
}

struct ResourceInfo {
    group: Option<u32>,
    binding: Option<u32>,
    space: Option<SpaceKind>,
}

pub(super) fn bind_globals(ctx: &Context, function: &mut Function, env: &mut Env) {
    for g in &ctx.globals {
        let pointer = function
            .expressions
            .append(Expression::GlobalVariable(g.handle), Span::UNDEFINED);
        env.push_rw(g.name.clone(), Slot::Ptr(pointer), g.ty, g.writable);
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
    let group = info
        .group
        .ok_or_else(|| Error::MissingResourceBinding(name.clone()))?;
    let binding = info
        .binding
        .ok_or_else(|| Error::MissingResourceBinding(name.clone()))?;

    if ctx.globals.iter().any(|g| g.name == name) {
        return Err(Error::DuplicateGlobal(name));
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
    };

    let handle = ctx.module.global_variables.append(
        GlobalVariable {
            name: Some(name.clone()),
            space,
            binding: Some(ResourceBinding { group, binding }),
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

fn parse_resource_attrs(attrs: &[Attribute]) -> Result<ResourceInfo, Error> {
    let mut info = ResourceInfo {
        group: None,
        binding: None,
        space: None,
    };
    for attr in attrs {
        if attr.path().is_ident("group") {
            if info.group.is_some() {
                return Err(Error::ConflictingStage);
            }
            info.group = Some(parse_u32_arg(attr, "group")?);
        } else if attr.path().is_ident("binding") {
            if info.binding.is_some() {
                return Err(Error::ConflictingStage);
            }
            info.binding = Some(parse_u32_arg(attr, "binding")?);
        } else if attr.path().is_ident("uniform") {
            set_space(&mut info.space, SpaceKind::Uniform)?;
        } else if attr.path().is_ident("storage") {
            let write = parse_storage_write(attr)?;
            set_space(&mut info.space, SpaceKind::Storage { write })?;
        }
    }
    Ok(info)
}

fn set_space(slot: &mut Option<SpaceKind>, space: SpaceKind) -> Result<(), Error> {
    if slot.is_some() {
        return Err(Error::ConflictingStage);
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
