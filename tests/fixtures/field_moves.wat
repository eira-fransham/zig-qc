;; How LLVM moves entity fields once they are struct fields (vanilla defs:
;; `movetype` 8, `solid` 9, `origin` 10, `frame` 30, `think` 44, `enemy` 75).
(module
  (import "env" "memory" (memory 1))
  (import "env" "__table_base" (global $tb i32))
  (import "env" "__indirect_function_table" (table 1 funcref))
  (elem (global.get $tb) func $noop)
  (func $noop)

  ;; Two adjacent float fields set at once, as one `i64` of their bits:
  ;; `movetype = 1`, `solid = 2`.
  (func (export "set_pair") (param $e i32)
    (i64.store offset=32 align=4 (local.get $e) (i64.const 0x400000003f800000)))

  ;; Two adjacent fields copied at once (`origin_x` and `origin_y`), loaded
  ;; before a store that might overwrite them.
  (func (export "copy_xy") (param $a i32) (param $b i32)
    (local $xy i64)
    (local.set $xy (i64.load offset=40 align=4 (local.get $b)))
    (f32.store offset=40 (local.get $b) (f32.const 0))
    (i64.store offset=40 align=4 (local.get $a) (local.get $xy)))

  ;; A float field copied as an `i32`.
  (func (export "copy_frame") (param $a i32) (param $b i32)
    (i32.store offset=120 (local.get $a) (i32.load offset=120 (local.get $b))))

  ;; A float passed as its bits to a helper that only stores it.
  (func $set_frame (param $e i32) (param $bits i32)
    (i32.store offset=120 (local.get $e) (local.get $bits)))
  (func (export "frame_seven") (param $e i32)
    (call $set_frame (local.get $e) (i32.const 0x40e00000)))

  ;; One field loaded into every lane: `origin_z * (1, 2, 3)`.
  (func (export "splat_z") (param $e i32) (result v128)
    (f32x4.mul
      (v128.load32_splat offset=48 (local.get $e))
      (v128.const f32x4 1 2 3 0)))

  ;; Stores to different fields on each side of a branch, sunk into one
  ;; store at `e + phi(300, 176)`: `c ? (e.enemy = o) : (e.think = noop)`.
  (func (export "sunk_store") (param $e i32) (param $o i32) (param $c f32)
    (local $off i32) (local $v i32)
    (if (f32.ne (local.get $c) (f32.const 0))
      (then (local.set $off (i32.const 300)) (local.set $v (local.get $o)))
      (else (local.set $off (i32.const 176)) (local.set $v (global.get $tb))))
    (i32.store (i32.add (local.get $e) (local.get $off)) (local.get $v)))

  ;; A comparison mask widened to `i64x2`, of which the low half of lane 1
  ;; (lane 1 of the mask) is used: `v_y < 0 ? 1 : 2`.
  (func (export "widened_mask") (param $v v128) (result f32)
    (select (f32.const 1) (f32.const 2)
      (i32.and
        (i32x4.extract_lane 2
          (i64x2.extend_low_i32x4_s
            (f32x4.lt (local.get $v) (v128.const f32x4 0 0 0 0))))
        (i32.const 1)))))
