use naga::{
    Binding, BuiltIn, EntryPoint, Function, FunctionResult, Interpolation, ScalarKind, ShaderStage,
    TypeInner,
};
use syn::{Attribute, FnArg, ItemFn, LitInt, Meta, ReturnType};

use super::env::Env;
use super::{lower_signature, Context};
use crate::Error;

#[derive(Default)]
pub(super) struct StageInfo {
    pub stage: Option<ShaderStage>,
    pub workgroup_size: Option<[u32; 3]>,
    pub return_binding: Option<Binding>,
}

pub(super) fn parse_fn_attrs(attrs: &[Attribute]) -> Result<StageInfo, Error> {
    let mut info = StageInfo::default();
    for attr in attrs {
        if attr.path().is_ident("vertex") {
            set_stage(&mut info.stage, ShaderStage::Vertex)?;
        } else if attr.path().is_ident("fragment") {
            set_stage(&mut info.stage, ShaderStage::Fragment)?;
        } else if attr.path().is_ident("compute") {
            set_stage(&mut info.stage, ShaderStage::Compute)?;
        } else if attr.path().is_ident("workgroup_size") {
            info.workgroup_size = Some(parse_workgroup_size(attr)?);
        } else if attr.path().is_ident("output") || attr.path().is_ident("return") {
            info.return_binding = Some(parse_binding_meta(attr)?);
        }
    }
    Ok(info)
}

fn set_stage(slot: &mut Option<ShaderStage>, stage: ShaderStage) -> Result<(), Error> {
    if slot.is_some() {
        return Err(Error::ConflictingStage);
    }
    *slot = Some(stage);
    Ok(())
}

fn parse_workgroup_size(attr: &Attribute) -> Result<[u32; 3], Error> {
    let lits = attr
        .parse_args_with(syn::punctuated::Punctuated::<LitInt, syn::Token![,]>::parse_terminated)
        .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
    if lits.is_empty() || lits.len() > 3 {
        return Err(Error::UnsupportedBinding("workgroup_size".into()));
    }
    let mut size = [1u32, 1, 1];
    for (i, lit) in lits.iter().enumerate() {
        size[i] = lit
            .base10_parse()
            .map_err(|_| Error::UnsupportedBinding("workgroup_size".into()))?;
        if size[i] == 0 {
            return Err(Error::UnsupportedBinding("workgroup_size 0".into()));
        }
    }
    Ok(size)
}

fn parse_binding_meta(attr: &Attribute) -> Result<Binding, Error> {
    let meta: Meta = attr
        .parse_args()
        .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
    match meta {
        Meta::List(list) if list.path.is_ident("builtin") => {
            let ident: syn::Ident = list
                .parse_args()
                .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
            Ok(Binding::BuiltIn(map_builtin(&ident.to_string())?))
        }
        Meta::List(list) if list.path.is_ident("location") => {
            let lit: LitInt = list
                .parse_args()
                .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
            let location = lit
                .base10_parse()
                .map_err(|_| Error::UnsupportedBinding("location".into()))?;
            Ok(Binding::Location {
                location,
                interpolation: None,
                sampling: None,
                blend_src: None,
                per_primitive: false,
            })
        }
        _ => Err(Error::UnsupportedBinding("output".into())),
    }
}

fn parse_arg_binding(attrs: &[Attribute]) -> Result<Option<Binding>, Error> {
    let mut found = None;
    for attr in attrs {
        if attr.path().is_ident("builtin") || attr.path().is_ident("location") {
            if found.is_some() {
                return Err(Error::ConflictingStage);
            }
            found = Some(parse_plain_binding(attr)?);
        }
    }
    Ok(found)
}

fn parse_plain_binding(attr: &Attribute) -> Result<Binding, Error> {
    if attr.path().is_ident("builtin") {
        let ident: syn::Ident = attr
            .parse_args()
            .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
        Ok(Binding::BuiltIn(map_builtin(&ident.to_string())?))
    } else if attr.path().is_ident("location") {
        let lit: LitInt = attr
            .parse_args()
            .map_err(|e| Error::UnsupportedBinding(e.to_string()))?;
        let location = lit
            .base10_parse()
            .map_err(|_| Error::UnsupportedBinding("location".into()))?;
        Ok(Binding::Location {
            location,
            interpolation: None,
            sampling: None,
            blend_src: None,
            per_primitive: false,
        })
    } else {
        Err(Error::UnsupportedBinding("binding".into()))
    }
}

fn map_builtin(name: &str) -> Result<BuiltIn, Error> {
    Ok(match name {
        "position" => BuiltIn::Position { invariant: false },
        "vertex_index" => BuiltIn::VertexIndex,
        "instance_index" => BuiltIn::InstanceIndex,
        "global_invocation_id" => BuiltIn::GlobalInvocationId,
        "local_invocation_id" => BuiltIn::LocalInvocationId,
        "local_invocation_index" => BuiltIn::LocalInvocationIndex,
        "workgroup_id" => BuiltIn::WorkGroupId,
        "workgroup_size" => BuiltIn::WorkGroupSize,
        "num_workgroups" => BuiltIn::NumWorkGroups,
        "front_facing" => BuiltIn::FrontFacing,
        "frag_depth" => BuiltIn::FragDepth,
        other => return Err(Error::UnsupportedBinding(other.into())),
    })
}

fn is_unit(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Tuple(t) if t.elems.is_empty())
}

fn fill_interpolation(
    ctx: &Context,
    ty: naga::Handle<naga::Type>,
    stage: ShaderStage,
    is_input: bool,
    binding: &mut Binding,
) {
    let Binding::Location { interpolation, .. } = binding else {
        return;
    };
    let needs = matches!(
        (stage, is_input),
        (ShaderStage::Vertex, false) | (ShaderStage::Fragment, true)
    );
    if !needs || interpolation.is_some() {
        return;
    }
    let integer = match ctx.module.types[ty].inner {
        TypeInner::Scalar(s) | TypeInner::Vector { scalar: s, .. } => {
            matches!(s.kind, ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool)
        }
        _ => false,
    };
    *interpolation = Some(if integer {
        Interpolation::Flat
    } else {
        Interpolation::Perspective
    });
}

pub(super) fn lower_entry(
    ctx: &mut Context,
    item: ItemFn,
    info: StageInfo,
) -> Result<(), Error> {
    let stage = info.stage.expect("stage present");
    let name = item.sig.ident.to_string();

    if stage != ShaderStage::Compute && info.workgroup_size.is_some() {
        return Err(Error::UnexpectedWorkgroupSize);
    }
    let workgroup_size = if stage == ShaderStage::Compute {
        info.workgroup_size.ok_or(Error::MissingWorkgroupSize)?
    } else {
        [0, 0, 0]
    };

    let result = match &item.sig.output {
        ReturnType::Default => {
            if stage == ShaderStage::Compute {
                None
            } else {
                return Err(Error::MissingReturnType(name));
            }
        }
        ReturnType::Type(_, ty) if is_unit(ty) => {
            if stage == ShaderStage::Compute {
                None
            } else {
                return Err(Error::MissingReturnType(name));
            }
        }
        ReturnType::Type(_, ty) => {
            let result_ty = ctx.lower_type(ty)?;
            let mut binding = info
                .return_binding
                .ok_or_else(|| Error::MissingReturnBinding(name.clone()))?;
            fill_interpolation(ctx, result_ty, stage, false, &mut binding);
            Some(FunctionResult {
                ty: result_ty,
                binding: Some(binding),
            })
        }
    };

    let mut function = Function {
        name: Some(name),
        arguments: Vec::new(),
        result,
        ..Default::default()
    };

    let mut env = Env::default();
    lower_signature(ctx, &mut function, &item.sig, &mut env)?;

    for (arg, fn_arg) in function.arguments.iter_mut().zip(item.sig.inputs.iter()) {
        let FnArg::Typed(pat_ty) = fn_arg else {
            return Err(Error::Receiver);
        };
        let name = arg.name.clone().unwrap_or_default();
        let mut binding = parse_arg_binding(&pat_ty.attrs)?
            .ok_or_else(|| Error::MissingArgBinding(name))?;
        fill_interpolation(ctx, arg.ty, stage, true, &mut binding);
        arg.binding = Some(binding);
    }

    let mut body = naga::Block::new();
    env.push_scope();
    let tail = super::stmt::lower_block(ctx, &mut function, &mut body, &item.block, &mut env)?;
    env.pop_scope();
    if let Some((value, _)) = tail {
        body.push(
            naga::Statement::Return { value: Some(value) },
            naga::Span::UNDEFINED,
        );
    }
    function.body = body;

    ctx.module.entry_points.push(EntryPoint {
        name: function.name.clone().unwrap_or_default(),
        stage,
        early_depth_test: None,
        workgroup_size,
        workgroup_size_overrides: None,
        function,
        mesh_info: None,
        task_payload: None,
        incoming_ray_payload: None,
    });
    Ok(())
}
