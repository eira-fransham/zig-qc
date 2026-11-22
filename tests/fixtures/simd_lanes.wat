;; SIMD that LLVM generates for vector code: lane shuffles, and lane-wise
;; comparisons whose masks feed `bitselect`, `not` and `extract_lane`.
(module
  (import "env" "__quakec_load_vector" (func $load_vec (param i32) (result v128)))
  (import "env" "__quakec_store_vector" (func $store_vec (param i32 v128)))
  ;; `v_up = v_forward * v_forward_x`, with the splat done by a shuffle.
  (func (export "splat_x")
    (local $v v128)
    (local.set $v (call $load_vec (i32.const 236)))
    (call $store_vec (i32.const 248)
      (f32x4.mul (local.get $v)
        (i8x16.shuffle 0 1 2 3 0 1 2 3 0 1 2 3 0 1 2 3 (local.get $v) (local.get $v)))))
  ;; `v_up = [v_forward_x, v_up_y, v_up_z]`.
  (func (export "blend")
    (call $store_vec (i32.const 248)
      (i8x16.shuffle 0 1 2 3 20 21 22 23 24 25 26 27 0 1 2 3
        (call $load_vec (i32.const 236)) (call $load_vec (i32.const 248)))))
  ;; Lane-wise maximum by comparison, `pmin`/`pmax`, and testing a mask lane.
  (func (export "masks") (result i32)
    (local $a v128) (local $b v128) (local $m v128)
    (local.set $a (call $load_vec (i32.const 236)))
    (local.set $b (call $load_vec (i32.const 248)))
    (local.set $m (f32x4.gt (local.get $a) (local.get $b)))
    (call $store_vec (i32.const 236) (v128.bitselect (local.get $a) (local.get $b) (local.get $m)))
    (call $store_vec (i32.const 248)
      (f32x4.pmin (local.get $a) (f32x4.pmax (local.get $a) (local.get $b))))
    (i32.and (i32x4.extract_lane 1 (v128.not (local.get $m))) (i32.const 1))))
