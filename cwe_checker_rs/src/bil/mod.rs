use crate::intermediate_representation::BinOpType as IrBinOpType;
use crate::intermediate_representation::ByteSize;
use crate::intermediate_representation::CastOpType as IrCastOpType;
use crate::intermediate_representation::Expression as IrExpression;
use crate::intermediate_representation::UnOpType as IrUnOpType;
use apint::Width;
use serde::{Deserialize, Serialize};

pub mod variable;
pub use variable::*;

pub type Bitvector = apint::ApInt;

pub type BitSize = u16;

impl From<BitSize> for ByteSize {
    /// Convert to `ByteSize`, while always rounding up to the nearest full byte.
    fn from(bitsize: BitSize) -> ByteSize {
        ((bitsize as u64 + 7) / 8).into()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum Expression {
    Var(Variable),
    Const(Bitvector),
    Load {
        memory: Box<Expression>,
        address: Box<Expression>,
        endian: Endianness,
        size: BitSize,
    },
    Store {
        memory: Box<Expression>,
        address: Box<Expression>,
        value: Box<Expression>,
        endian: Endianness,
        size: BitSize,
    },
    BinOp {
        op: BinOpType,
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    UnOp {
        op: UnOpType,
        arg: Box<Expression>,
    },
    Cast {
        kind: CastType,
        width: BitSize,
        arg: Box<Expression>,
    },
    Let {
        var: Variable,
        bound_exp: Box<Expression>,
        body_exp: Box<Expression>,
    },
    Unknown {
        description: String,
        type_: Type,
    },
    IfThenElse {
        condition: Box<Expression>,
        true_exp: Box<Expression>,
        false_exp: Box<Expression>,
    },
    Extract {
        low_bit: BitSize,
        high_bit: BitSize,
        arg: Box<Expression>,
    },
    Concat {
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

impl Expression {
    /// Resolve all let-bindings inside an expression to create an equivalent expression without usage of let-bindings.
    pub fn replace_let_bindings(&mut self) {
        use Expression::*;
        match self {
            Var(_) | Const(_) | Unknown { .. } => (),
            Load {
                memory, address, ..
            } => {
                memory.replace_let_bindings();
                address.replace_let_bindings();
            }
            Store {
                memory,
                address,
                value,
                ..
            } => {
                memory.replace_let_bindings();
                address.replace_let_bindings();
                value.replace_let_bindings();
            }
            BinOp { op: _, lhs, rhs } => {
                lhs.replace_let_bindings();
                rhs.replace_let_bindings();
            }
            UnOp { op: _, arg } => arg.replace_let_bindings(),
            Cast {
                kind: _,
                width: _,
                arg,
            } => arg.replace_let_bindings(),
            Let {
                var,
                bound_exp,
                body_exp,
            } => {
                let to_replace = Expression::Var(var.clone());
                body_exp.replace_let_bindings();
                body_exp.substitute(&to_replace, bound_exp);
                *self = *body_exp.clone();
            }
            IfThenElse {
                condition,
                true_exp,
                false_exp,
            } => {
                condition.replace_let_bindings();
                true_exp.replace_let_bindings();
                false_exp.replace_let_bindings();
            }
            Extract {
                low_bit: _,
                high_bit: _,
                arg,
            } => arg.replace_let_bindings(),
            Concat { left, right } => {
                left.replace_let_bindings();
                right.replace_let_bindings();
            }
        }
    }

    /// Substitutes all subexpressions equal to `to_replace` with the expression `replace_with`.
    fn substitute(&mut self, to_replace: &Expression, replace_with: &Expression) {
        use Expression::*;
        if self == to_replace {
            *self = replace_with.clone();
        } else {
            match self {
                Var(_) | Const(_) | Unknown { .. } => (),
                Load {
                    memory, address, ..
                } => {
                    memory.substitute(to_replace, replace_with);
                    address.substitute(to_replace, replace_with);
                }
                Store {
                    memory,
                    address,
                    value,
                    ..
                } => {
                    memory.substitute(to_replace, replace_with);
                    address.substitute(to_replace, replace_with);
                    value.substitute(to_replace, replace_with);
                }
                BinOp { op: _, lhs, rhs } => {
                    lhs.substitute(to_replace, replace_with);
                    rhs.substitute(to_replace, replace_with);
                }
                UnOp { op: _, arg } => arg.substitute(to_replace, replace_with),
                Cast {
                    kind: _,
                    width: _,
                    arg,
                } => arg.substitute(to_replace, replace_with),
                Let {
                    var: _,
                    bound_exp,
                    body_exp,
                } => {
                    bound_exp.substitute(to_replace, replace_with);
                    body_exp.substitute(to_replace, replace_with);
                }
                IfThenElse {
                    condition,
                    true_exp,
                    false_exp,
                } => {
                    condition.substitute(to_replace, replace_with);
                    true_exp.substitute(to_replace, replace_with);
                    false_exp.substitute(to_replace, replace_with);
                }
                Extract {
                    low_bit: _,
                    high_bit: _,
                    arg,
                } => arg.substitute(to_replace, replace_with),
                Concat { left, right } => {
                    left.substitute(to_replace, replace_with);
                    right.substitute(to_replace, replace_with);
                }
            }
        }
    }

    pub fn bitsize(&self) -> BitSize {
        use Expression::*;
        match self {
            Var(var) => var.bitsize().unwrap(),
            Const(bitvector) => bitvector.width().to_usize() as u16,
            Load { size, .. } => *size,
            Store { .. } => 0,
            BinOp { op, lhs, rhs: _ } => {
                use BinOpType::*;
                match op {
                    EQ | NEQ | LT | LE | SLT | SLE => 1,
                    _ => lhs.bitsize(),
                }
            }
            UnOp { arg, .. } => arg.bitsize(),
            Cast { width, .. } => *width,
            Let { .. } => panic!(),
            Unknown {
                description: _,
                type_,
            } => type_.bitsize().unwrap(),
            IfThenElse { true_exp, .. } => true_exp.bitsize(),
            Extract {
                low_bit, high_bit, ..
            } => high_bit - low_bit,
            Concat { left, right } => left.bitsize() + right.bitsize(),
        }
    }
}

impl From<Expression> for IrExpression {
    fn from(expr: Expression) -> IrExpression {
        use Expression::*;
        match expr {
            Var(var) => IrExpression::Var(var.into()),
            Const(bitvector) => IrExpression::Const(bitvector),
            Load { .. } | Store { .. } | Let { .. } | Unknown { .. } | IfThenElse { .. } => {
                panic!()
            }
            BinOp { op, lhs, rhs } => IrExpression::BinOp {
                op: op.into(),
                lhs: Box::new(IrExpression::from(*lhs)),
                rhs: Box::new(IrExpression::from(*rhs)),
            },
            UnOp { op, arg } => IrExpression::UnOp {
                op: op.into(),
                arg: Box::new(IrExpression::from(*arg)),
            },
            Cast { kind, width, arg } => {
                use CastType::*;
                match kind {
                    UNSIGNED => IrExpression::Cast {
                        arg: Box::new(IrExpression::from(*arg)),
                        op: IrCastOpType::IntZExt,
                        size: width.into(),
                    },
                    SIGNED => IrExpression::Cast {
                        arg: Box::new(IrExpression::from(*arg)),
                        op: IrCastOpType::IntSExt,
                        size: width.into(),
                    },
                    HIGH => {
                        assert!(width % 8 == 0);
                        let low_byte = (arg.bitsize() - BitSize::from(width)).into();
                        IrExpression::Subpiece {
                            arg: Box::new(IrExpression::from(*arg)),
                            low_byte,
                            size: width.into(),
                        }
                    }
                    LOW => IrExpression::Subpiece {
                        arg: Box::new(IrExpression::from(*arg)),
                        low_byte: (0 as u64).into(),
                        size: width.into(),
                    },
                }
            }
            Extract {
                low_bit,
                high_bit,
                arg,
            } => IrExpression::Subpiece {
                size: (high_bit - low_bit + 1).into(),
                low_byte: low_bit.into(),
                arg: Box::new(IrExpression::from(*arg)),
            },
            Concat { left, right } => IrExpression::BinOp {
                op: IrBinOpType::Piece,
                lhs: Box::new(IrExpression::from(*left)),
                rhs: Box::new(IrExpression::from(*right)),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum CastType {
    UNSIGNED,
    SIGNED,
    HIGH,
    LOW,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum BinOpType {
    PLUS,
    MINUS,
    TIMES,
    DIVIDE,
    SDIVIDE,
    MOD,
    SMOD,
    LSHIFT,
    RSHIFT,
    ARSHIFT,
    AND,
    OR,
    XOR,
    EQ,
    NEQ,
    LT,
    LE,
    SLT,
    SLE,
}

impl From<BinOpType> for IrBinOpType {
    fn from(op: BinOpType) -> IrBinOpType {
        use BinOpType::*;
        use IrBinOpType::*;
        match op {
            PLUS => IntAdd,
            MINUS => IntSub,
            TIMES => IntMult,
            DIVIDE => IntDiv,
            SDIVIDE => IntSDiv,
            MOD => IntRem,
            SMOD => IntSRem,
            LSHIFT => IntLeft,
            RSHIFT => IntRight,
            ARSHIFT => IntSRight,
            AND => IntAnd,
            OR => IntOr,
            XOR => IntXOr,
            EQ => IntEqual,
            NEQ => IntNotEqual,
            LT => IntLess,
            LE => IntLessEqual,
            SLT => IntSLess,
            SLE => IntSLessEqual,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum UnOpType {
    NEG,
    NOT,
}

impl From<UnOpType> for IrUnOpType {
    fn from(op: UnOpType) -> IrUnOpType {
        use UnOpType::*;
        match op {
            NEG => IrUnOpType::Int2Comp,
            NOT => IrUnOpType::IntNegate,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Endianness {
    LittleEndian,
    BigEndian,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register(name: &str) -> Variable {
        Variable {
            name: name.into(),
            type_: Type::Immediate(64),
            is_temp: false,
        }
    }

    #[test]
    fn variant_deserialization() {
        let string = "\"UNSIGNED\"";
        assert_eq!(CastType::UNSIGNED, serde_json::from_str(string).unwrap());
        let string = "\"NEG\"";
        assert_eq!(UnOpType::NEG, serde_json::from_str(string).unwrap());
    }

    #[test]
    fn bitvector_deserialization() {
        let bitv = Bitvector::from_u64(234);
        let string = serde_json::to_string(&bitv).unwrap();
        println!("{}", string);
        println!("{:?}", bitv);
        let string = "{\"digits\":[234],\"width\":[64]}";
        assert_eq!(bitv, serde_json::from_str(string).unwrap());
    }

    #[test]
    fn expression_deserialization() {
        let string = "{\"BinOp\":{\"lhs\":{\"Const\":{\"digits\":[234],\"width\":[8]}},\"op\":\"PLUS\",\"rhs\":{\"Const\":{\"digits\":[234],\"width\":[8]}}}}";
        let bitv = Bitvector::from_u8(234);
        let exp = Expression::BinOp {
            op: BinOpType::PLUS,
            lhs: Box::new(Expression::Const(bitv.clone())),
            rhs: Box::new(Expression::Const(bitv)),
        };
        println!("{}", serde_json::to_string(&exp).unwrap());
        assert_eq!(exp, serde_json::from_str(string).unwrap())
    }

    #[test]
    fn replace_let_bindings() {
        let mut source_exp = Expression::Let {
            var: register("x"),
            bound_exp: Box::new(Expression::Const(Bitvector::from_u64(12))),
            body_exp: Box::new(Expression::BinOp {
                op: BinOpType::PLUS,
                lhs: Box::new(Expression::Var(register("x"))),
                rhs: Box::new(Expression::Const(Bitvector::from_u64(42))),
            }),
        };
        let target_exp = Expression::BinOp {
            op: BinOpType::PLUS,
            lhs: Box::new(Expression::Const(Bitvector::from_u64(12))),
            rhs: Box::new(Expression::Const(Bitvector::from_u64(42))),
        };

        source_exp.replace_let_bindings();
        assert_eq!(source_exp, target_exp);
    }
}