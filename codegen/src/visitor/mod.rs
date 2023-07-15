//! This module is the central place for machine code emission.
//!
//! It defines an implementation of wasmparser's Visitor trait for
//! `CodeGen`; which defines a visitor per op-code, which validates
//! and dispatches to the corresponding machine code emitter.

use crate::{CodeGen, Result};
use paste::paste;
use tracing::trace;
use wasmparser::{for_each_operator, BlockType, BrTable, Ieee32, Ieee64, MemArg, VisitOperator};

mod control;
mod local;
mod system;

/// A macro to define unsupported WebAssembly operators.
///
/// This macro calls itself recursively;
/// 1. It no-ops when matching a supported operator.
/// 2. Defines the visitor function and panics when
/// matching an unsupported operator.
macro_rules! impl_visit_operator {
    ( @mvp $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident $($rest:tt)* ) => {
        impl_visit_operator!($($rest)*);
    };
    ( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident $($rest:tt)* ) => {
        fn $visit(&mut self $($(, $arg: $argty)*)?) -> Self::Output {
            trace!("{}", stringify!($op));
            Ok(())
        }

        impl_visit_operator!($($rest)*);
    };
    () => {};
}

/// Implement arithmetic operators for types.
macro_rules! map_wasm_operators {
    (@basic $ty:tt, $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        paste! {
            fn [< visit_ $ty _ $wasm >](&mut self $($(,$arg: $argty),* )?) -> Self::Output {
                trace!("{}.{}", stringify!($ty), stringify!($evm));
                self.masm.[< _ $evm >]()?;

                Ok(())
            }
        }
    };
    (@integer32 $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@basic i32, $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@integer64 $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@basic i64, $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@signed $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@integer32 $wasm, $evm $(, { $($arg: $argty),* })?);
        map_wasm_operators!(@integer64 $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@integer $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        paste!{
            map_wasm_operators!(@signed [< $wasm _s >], $evm $(, { $($arg: $argty),* })?);
            map_wasm_operators!(@signed [< $wasm _u >], $evm $(, { $($arg: $argty),* })?);
        }
    };
    (@float32 $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@basic f32, $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@float64 $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@basic f64, $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@float $wasm:tt, $evm:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@float32 $wasm, $evm $(, { $($arg: $argty),* })?);
        map_wasm_operators!(@float64 $wasm, $evm $(, { $($arg: $argty),* })?);
    };
    (@signed_and_float $op:tt $(, { $($arg:ident: $argty:ty),* })?) => {
        map_wasm_operators!(@signed $op, $op);
        map_wasm_operators!(@float $op, $op);
    };
    (@field ($($field:ident).*) $op:tt $($arg:tt: $argty:ty),* ) => {
        paste! {
            fn [< visit_ $op >](&mut self, $($arg: $argty),*) -> Self::Output {
                trace!("{}", stringify!($op));
                self.$($field.)*[< _ $op >]($($arg),*)?;

                Ok(())
            }
        }
    };
    (
        xdr: [$($xdr:tt),+],
        signed: [$($signed:tt),+],
        integer: [$($integer:tt),+],
        float: [$($float:tt),+],
        signed_and_float: [$($op:tt),+],
        map: {
            all: [$($wasm:tt => $evm:tt),+],
            integer: [$($map_int_wasm:tt => $map_int_evm:tt),+],
        },
        mem: {
            all: [$($mem:tt),+],
            integer: [$($mem_integer:tt),+],
            integer64: [$($mem_integer64:tt),+],
            signed: [$($mem_signed:tt),+],
            signed64: [$($mem_signed64:tt),+],
            signed_and_float: [$($mem_signed_and_float:tt),+],
        },
        masm: {
            $( $masm:tt $(: { $($marg:ident: $margty:ty),+ })? ),+
        },
        global: {
            $( $global:tt $(: { $($garg:ident: $gargty:ty),+ })? ),+
        }
    ) => {
        paste! {
            $(map_wasm_operators!(@signed_and_float $op);)+

            $(
                map_wasm_operators!(@signed [< $xdr _s >], [< s $xdr >]);
                map_wasm_operators!(@signed [< $xdr _u >], $xdr);
                map_wasm_operators!(@float $xdr, $xdr);
            )+

            $(map_wasm_operators!(@signed $signed, $signed);)+
            $(map_wasm_operators!(@integer $integer, $integer);)+
            $(map_wasm_operators!(@float $float, $float);)+

            $(
                map_wasm_operators!(@integer $wasm, $evm);
                map_wasm_operators!(@float $wasm, $evm);
            )+

            $(
                map_wasm_operators!(@signed [< $map_int_wasm _s >], [< s $map_int_evm >]);
                map_wasm_operators!(@signed [< $map_int_wasm _u >], $map_int_evm);
            )+

            $(
                map_wasm_operators!(@signed $mem, $mem, { _arg: MemArg });
                map_wasm_operators!(@float $mem, $mem, { _arg: MemArg });
            )+


            $(
                map_wasm_operators!(@integer $mem_integer, $mem_integer, { _arg: MemArg });
            )+

            $(
                map_wasm_operators!(@integer64 [< $mem_integer64 _s >], $mem_integer64, { _arg: MemArg });
                map_wasm_operators!(@integer64 [< $mem_integer64 _u >], $mem_integer64, { _arg: MemArg });
            )+


            $(
                map_wasm_operators!(@signed $mem_signed, $mem_signed, { _arg: MemArg });
            )+

            $(
                map_wasm_operators!(@signed $mem_signed_and_float, $mem_signed_and_float, { _arg: MemArg });
                map_wasm_operators!(@float $mem_signed_and_float, $mem_signed_and_float, { _arg: MemArg });
            )+

            $(
                map_wasm_operators!(@integer64 $mem_signed64, $mem_signed64, { _arg: MemArg });
            )+

            $(
                map_wasm_operators!(@field (masm) $masm $( $($marg: $margty),+ )?);
            )+

            $(
                map_wasm_operators!(@field () $global $( $($garg: $gargty),+ )?);
            )+
        }
    };
}

impl<'a> VisitOperator<'a> for CodeGen {
    type Output = Result<()>;

    for_each_operator!(impl_visit_operator);

    map_wasm_operators! {
        xdr: [div, lt, gt],
        signed: [and, clz, ctz, eqz, or, popcnt, rotl, rotr, shl, xor],
        integer: [shr, trunc_f32, trunc_f64],
        float: [
            abs, ceil, copysign, floor, max, min, nearest, neg, sqrt,
            convert_i32_s, convert_i32_u, convert_i64_s, convert_i64_u,
            trunc
        ],
        signed_and_float: [add, sub, mul, eq, ne],
        map: {
            all: [ge => sgt, le => slt],
            integer: [rem => mod],
        },
        mem: {
            all: [load],
            integer: [load8, load16],
            integer64: [load32],
            signed: [store8, store16],
            signed64: [store32],
            signed_and_float: [store],
        },
        masm: {
            drop,
            memory_grow: {
                mem: u32,
                mem_byte: u8
            },
            memory_size: {
                mem: u32,
                mem_byte: u8
            },
            i32_const: {
                value: i32
            },
            i64_const: {
                value: i64
            },
            f32_const: {
                value: Ieee32
            },
            f64_const: {
                value: Ieee64
            },
            i32_wrap_i64,
            i64_extend_i32_s,
            i64_extend_i32_u,
            f32_demote_f64,
            f64_promote_f32,
            i32_reinterpret_f32,
            i64_reinterpret_f64,
            f32_reinterpret_i32,
            f64_reinterpret_i64,
            return
        },
        global: {
            else, select, end, nop, unreachable,
            if: {
                blockty: BlockType
            },
            block: {
                blockty: BlockType
            },
            loop: {
                blockty: BlockType
            },
            br: {
                relative_depth: u32
            },
            br_if: {
                relative_depth: u32
            },
            br_table: {
                table: BrTable<'_>
            },
            local_get: {
                local_index: u32
            },
            local_set: {
                local_index: u32
            },
            local_tee: {
                local_index: u32
            },
            global_get: {
                global_index: u32
            },
            global_set: {
                global_index: u32
            },
            call: {
                func_index: u32
            },
            call_indirect: {
                type_index: u32,
                table_index: u32,
                table_byte: u8
            }
        }
    }
}