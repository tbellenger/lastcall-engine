use crate::error::{Error, Result};
use crate::model::CmpOp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cond {
    Has(String),
    Flag(String),
    Room(String),
    Var {
        name: String,
        modulus: Option<i32>,
        op: CmpOp,
        rhs: i32,
    },
    Count {
        name: String,
        op: CmpOp,
        rhs: i32,
    },
    Not(Box<Cond>),
}

pub fn parse_cond(text: &str) -> Result<Cond> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('!') {
        return Ok(Cond::Not(Box::new(parse_cond(rest)?)));
    }
    if let Some(id) = text.strip_prefix("has:") {
        return ident(id).map(Cond::Has);
    }
    if let Some(id) = text.strip_prefix("flag:") {
        return ident(id).map(Cond::Flag);
    }
    if let Some(id) = text.strip_prefix("room:") {
        return ident(id).map(Cond::Room);
    }
    if let Some(expr) = text.strip_prefix("var:") {
        return parse_var(expr);
    }
    if let Some(expr) = text.strip_prefix("count:") {
        return parse_count(expr);
    }
    Err(Error::msg(format!("bad condition '{text}'")))
}

fn ident(text: &str) -> Result<String> {
    if is_ident(text) {
        Ok(text.to_string())
    } else {
        Err(Error::msg(format!("bad identifier '{text}'")))
    }
}

pub fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && !text.is_empty()
}

fn parse_var(expr: &str) -> Result<Cond> {
    let (name, rest) = split_ident(expr)?;
    if let Some(rest) = rest.strip_prefix('%') {
        let (modulus, cmp) = split_at_op(rest)?;
        let modulus: i32 = modulus
            .parse()
            .map_err(|_| Error::msg(format!("bad modulus in '{expr}'")))?;
        if modulus == 0 {
            return Err(Error::msg("modulus must not be 0"));
        }
        let (op, rhs) = parse_op(cmp)?;
        return Ok(Cond::Var {
            name,
            modulus: Some(modulus),
            op,
            rhs,
        });
    }
    let (op, rhs) = parse_op(rest)?;
    Ok(Cond::Var {
        name,
        modulus: None,
        op,
        rhs,
    })
}

fn parse_count(expr: &str) -> Result<Cond> {
    let (name, rest) = split_ident(expr)?;
    if rest.is_empty() {
        return Ok(Cond::Count {
            name,
            op: CmpOp::Ge,
            rhs: 1,
        });
    }
    let (op, rhs) = parse_op(rest)?;
    Ok(Cond::Count { name, op, rhs })
}

fn split_ident(expr: &str) -> Result<(String, &str)> {
    let end = expr
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(expr.len());
    let name = &expr[..end];
    if !is_ident(name) {
        return Err(Error::msg(format!("bad identifier in '{expr}'")));
    }
    Ok((name.to_string(), &expr[end..]))
}

fn split_at_op(expr: &str) -> Result<(&str, &str)> {
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(idx) = expr.find(op) {
            return Ok((&expr[..idx], &expr[idx..]));
        }
    }
    Err(Error::msg(format!("missing comparison in '{expr}'")))
}

fn parse_op(expr: &str) -> Result<(CmpOp, i32)> {
    let (op, rest) = if let Some(rest) = expr.strip_prefix("==") {
        (CmpOp::Eq, rest)
    } else if let Some(rest) = expr.strip_prefix("!=") {
        (CmpOp::Ne, rest)
    } else if let Some(rest) = expr.strip_prefix(">=") {
        (CmpOp::Ge, rest)
    } else if let Some(rest) = expr.strip_prefix("<=") {
        (CmpOp::Le, rest)
    } else if let Some(rest) = expr.strip_prefix('>') {
        (CmpOp::Gt, rest)
    } else if let Some(rest) = expr.strip_prefix('<') {
        (CmpOp::Lt, rest)
    } else {
        return Err(Error::msg(format!("bad comparison '{expr}'")));
    };
    let rhs: i32 = rest
        .trim()
        .parse()
        .map_err(|_| Error::msg(format!("bad number in '{expr}'")))?;
    Ok((op, rhs))
}

pub struct CondWorld<'a> {
    pub room: &'a str,
    pub flags: &'a std::collections::BTreeSet<String>,
    pub vars: &'a std::collections::BTreeMap<String, i32>,
    pub inventory: &'a std::collections::BTreeMap<String, i32>,
}

pub fn eval(world: &CondWorld<'_>, cond: &Cond) -> bool {
    match cond {
        Cond::Has(id) => world.inventory.get(id).copied().unwrap_or(0) > 0,
        Cond::Flag(id) => world.flags.contains(id),
        Cond::Room(id) => world.room == id,
        Cond::Var {
            name,
            modulus,
            op,
            rhs,
        } => {
            let mut value = world.vars.get(name).copied().unwrap_or(0);
            if let Some(m) = modulus {
                value %= *m;
            }
            op.eval(value, *rhs)
        }
        Cond::Count { name, op, rhs } => {
            let value = world.inventory.get(name).copied().unwrap_or(0);
            op.eval(value, *rhs)
        }
        Cond::Not(inner) => !eval(world, inner),
    }
}

pub fn eval_str(world: &CondWorld<'_>, text: &str) -> Result<bool> {
    Ok(eval(world, &parse_cond(text)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn parses_var_modulus() {
        let cond = parse_cond("var:thugTurn%3==2").unwrap();
        match cond {
            Cond::Var {
                name,
                modulus,
                op,
                rhs,
            } => {
                assert_eq!(name, "thugTurn");
                assert_eq!(modulus, Some(3));
                assert_eq!(op, CmpOp::Eq);
                assert_eq!(rhs, 2);
            }
            other => panic!("{other:?}"),
        }
        let mut vars = BTreeMap::new();
        vars.insert("thugTurn".into(), 5);
        let world = CondWorld {
            room: "bar",
            flags: &BTreeSet::new(),
            vars: &vars,
            inventory: &BTreeMap::new(),
        };
        assert!(eval(&world, &parse_cond("var:thugTurn%3==2").unwrap()));
        assert!(eval(&world, &parse_cond("!flag:open").unwrap()));
    }
}
