//! Structural type-alias resolution.
//!
//! `type Name = <Type>` declarations are erased before type-checking and codegen:
//! every `Type::Named(n)` whose `n` is a known alias is rewritten to the alias's
//! underlying type. Aliases referring to other aliases are flattened to a fixpoint.
//! Real class/net names (no alias entry) pass through untouched.

use std::collections::HashMap;

use crate::{span::Spanned, stmt::Stmt, types::Type};

/// Collect the aliases declared in `stmts` into `map`, then rewrite every type in
/// `stmts` against the accumulated `map`. `map` is shared across compilation units
/// so a module's aliases are visible to files that `use` it.
pub fn resolve_aliases<'a>(stmts: &mut [Spanned<Stmt<'a>>], map: &mut HashMap<String, Type>) {
    for s in stmts.iter() {
        if let Stmt::TypeAlias { name, ty } = &s.node {
            map.insert(name.node.to_string(), ty.clone());
        }
    }
    flatten(map);
    for s in stmts.iter_mut() {
        walk_stmt(&mut s.node, map);
    }
}

/// Resolve alias-to-alias references so each map value is fully expanded.
fn flatten(map: &mut HashMap<String, Type>) {
    let keys: Vec<String> = map.keys().cloned().collect();
    // A pass per alias is enough to flatten any acyclic chain; the cap also breaks cycles.
    for _ in 0..keys.len() {
        let mut changed = false;
        for k in &keys {
            let mut v = map.get(k).unwrap().clone();
            subst(&mut v, map, k);
            if &v != map.get(k).unwrap() {
                map.insert(k.clone(), v);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Replace alias `Named(n)` nodes inside `ty`. `skip` guards a self-referential
/// alias during flattening (leave it as `Named` rather than loop forever).
fn subst(ty: &mut Type, map: &HashMap<String, Type>, skip: &str) {
    match ty {
        Type::Named(n) => {
            if n != skip {
                if let Some(target) = map.get(n) {
                    *ty = target.clone();
                }
            }
        }
        Type::Nullable(inner)
        | Type::Try(inner)
        | Type::Array(inner)
        | Type::Vec(inner)
        | Type::Set(inner)
        | Type::Channel(inner) => subst(inner, map, skip),
        Type::Map(k, v) => {
            subst(k, map, skip);
            subst(v, map, skip);
        }
        Type::Struct(fields) => {
            for (_, fty) in fields {
                subst(fty, map, skip);
            }
        }
        Type::Func(args, ret) => {
            for a in args {
                subst(a, map, skip);
            }
            subst(ret, map, skip);
        }
        _ => {}
    }
}

fn resolve_opt(ty: &mut Option<Type>, map: &HashMap<String, Type>) {
    if let Some(t) = ty {
        subst(t, map, "");
    }
}

fn resolve_args(args: &mut [(Spanned<&str>, Type)], map: &HashMap<String, Type>) {
    for (_, t) in args {
        subst(t, map, "");
    }
}

fn walk_body<'a>(body: &mut [Spanned<Stmt<'a>>], map: &HashMap<String, Type>) {
    for s in body {
        walk_stmt(&mut s.node, map);
    }
}

fn walk_stmt<'a>(stmt: &mut Stmt<'a>, map: &HashMap<String, Type>) {
    match stmt {
        Stmt::Let { ty, .. } | Stmt::CopyLet { ty, .. } => resolve_opt(ty, map),
        Stmt::Fn { args, return_type, body, .. } => {
            resolve_args(args, map);
            resolve_opt(return_type, map);
            walk_body(body, map);
        }
        Stmt::AbstractFn { args, return_type, .. } => {
            resolve_args(args, map);
            resolve_opt(return_type, map);
        }
        Stmt::Init { args, body } => {
            resolve_args(args, map);
            walk_body(body, map);
        }
        Stmt::Default { body } => walk_body(body, map),
        Stmt::Spawn { body } => walk_body(body, map),
        Stmt::While { body, .. } => walk_body(body, map),
        Stmt::For { body, .. } => walk_body(body, map),
        Stmt::Enum { variants, .. } => {
            for (_, payload) in variants.iter_mut() {
                for t in payload {
                    subst(t, map, "");
                }
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms.iter_mut() {
                walk_body(&mut arm.body, map);
            }
        }
        Stmt::If { then, else_, .. } => {
            walk_body(then, map);
            if let Some(e) = else_ {
                walk_body(e, map);
            }
        }
        Stmt::IfLet { then, else_, .. } => {
            walk_body(then, map);
            if let Some(e) = else_ {
                walk_body(e, map);
            }
        }
        Stmt::Class { fields, methods, .. } => {
            for f in fields.iter_mut() {
                subst(&mut f.ty, map, "");
            }
            walk_body(methods, map);
        }
        Stmt::Interface { methods, .. } => {
            for m in methods.iter_mut() {
                for (_, t) in m.args.iter_mut() {
                    subst(t, map, "");
                }
                resolve_opt(&mut m.return_type, map);
            }
        }
        Stmt::Impl { methods, .. } => walk_body(methods, map),
        Stmt::TypeAlias { ty, .. } => subst(ty, map, ""),
        _ => {}
    }
}
