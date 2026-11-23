;; Code LLVM generates from ordinary Zig that needs special handling.
(module
  (import "env" "memory" (memory 1))
  (import "env" "__memory_base" (global $base i32))
  (import "env" "__quakec_string_literal" (func $lit (param i32) (result i32)))
  ;; +0: "axe", +4: "shotgun", +12: "none"
  (data (global.get $base) "axe\00shotgun\00none\00\00\00\00")
  ;; +20: a table of 2 string pointers, filled in by relocation.
  (data (offset (i32.add (global.get $base) (i32.const 20))) "\00\00\00\00\00\00\00\00")
  (func (export "__wasm_apply_data_relocs")
    (i32.store (i32.add (global.get $base) (i32.const 20)) (global.get $base))
    (i32.store (i32.add (global.get $base) (i32.const 24))
      (i32.add (global.get $base) (i32.const 4))))

  ;; A `switch` on 1 and 2: a range check of `w - 1` (unsigned, so 0 is
  ;; out of range), then a load from the pointer table.
  (func (export "weapon_name") (param $w f32) (result i32)
    (local $i i32)
    (local.set $i (i32.sub (i32.trunc_sat_f32_u (local.get $w)) (i32.const 1)))
    (call $lit
      (if (result i32) (i32.gt_u (local.get $i) (i32.const 1))
        (then (i32.add (global.get $base) (i32.const 12)))
        (else
          (i32.load offset=20
            (i32.add (global.get $base) (i32.shl (local.get $i) (i32.const 2))))))))

  ;; Unsigned comparisons of values of unknown sign.
  (func (export "lt_u") (param $a f32) (param $b f32) (result f32)
    (f32.convert_i32_u
      (i32.lt_u (i32.trunc_sat_f32_s (local.get $a)) (i32.trunc_sat_f32_s (local.get $b)))))

  ;; `usub.sat(x, 6)`: `x >=u x - 6` is `x >=u 6`.
  (func (export "sat_sub") (param $x f32) (result f32)
    (local $i i32) (local $d i32)
    (local.set $i (i32.trunc_sat_f32_s (local.get $x)))
    (local.set $d (i32.add (local.get $i) (i32.const -6)))
    (f32.convert_i32_s
      (select (local.get $d) (i32.const 0) (i32.ge_u (local.get $i) (local.get $d)))))

  ;; A loop counter from 0: known non-negative, so compared directly.
  (func (export "count_to") (param $n f32) (result f32)
    (local $i i32) (local $end i32)
    (local.set $end (i32.trunc_sat_f32_u (local.get $n)))
    (block $done
      (loop $l
        (br_if $done (i32.ge_u (local.get $i) (local.get $end)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $l)))
    (f32.convert_i32_u (local.get $i)))

  ;; Float sign tests on the bits, as in Zig's `@min`/`@max` (exact except
  ;; for -0, which QuakeC doesn't distinguish).
  (func (export "is_negative") (param $x f32) (result f32)
    (f32.convert_i32_u
      (i32.lt_s (i32.reinterpret_f32 (local.get $x)) (i32.const 0))))
  (func (export "signs_differ") (param $a f32) (param $b f32) (result f32)
    (f32.convert_i32_u
      (i32.lt_s
        (i32.xor (i32.reinterpret_f32 (local.get $a)) (i32.reinterpret_f32 (local.get $b)))
        (i32.const 0))))

  ;; One function under two names (LLVM merges identical functions): the
  ;; `nextthink` (field 46) of entity `x` or `y`.
  (func (export "pick_a") (export "pick_b") (param $c f32) (param $x i32) (param $y i32)
    (result f32)
    (f32.load offset=184
      (select (local.get $x) (local.get $y) (f32.ne (local.get $c) (f32.const 0))))))
