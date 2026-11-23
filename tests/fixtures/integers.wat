;; Integer operations LLVM emits that QuakeC has no instruction for.
(module
  ;; Division and remainder truncate toward zero.
  (func (export "div_s") (param i32 i32) (result i32)
    (i32.div_s (local.get 0) (local.get 1)))
  (func (export "rem_u") (param i32) (result i32)
    (i32.rem_u (local.get 0) (i32.const 7)))
  ;; `switch` membership as a bit test, in both forms.
  (func (export "is_30_day_month") (param i32) (result i32)
    (i32.ne (i32.and (i32.shl (i32.const 1) (local.get 0)) (i32.const 2640))
      (i32.const 0)))
  (func (export "bit_of") (param i32) (result i32)
    (i32.and (i32.shr_u (i32.const 5) (local.get 0)) (i32.const 1)))
  ;; Counters vectorized into `i32x4`, wrapped with a mask:
  ;; `v = v + 1; v = v & (v != [3, 5, 0, 0])`.
  (func (export "wrap") (param i32) (result i32)
    (local v128)
    (loop
      (local.set 1 (i32x4.add (local.get 1) (v128.const i32x4 1 1 1 1)))
      (local.set 1 (v128.and (local.get 1)
        (i32x4.ne (local.get 1) (v128.const i32x4 3 5 0 0))))
      (br_if 0 (local.tee 0 (i32.sub (local.get 0) (i32.const 1)))))
    (i32.add (i32x4.extract_lane 0 (local.get 1)) (i32x4.extract_lane 1 (local.get 1))))
  ;; Masks are all ones: `-mask` counts lanes.
  (func (export "count_lt") (param f32 f32 f32) (result i32)
    (local v128)
    (local.set 3 (i32x4.sub (v128.const i32x4 0 0 0 0)
      (f32x4.lt
        (f32x4.replace_lane 2 (f32x4.replace_lane 1 (f32x4.splat (local.get 0)) (local.get 1))
          (local.get 2))
        (v128.const f32x4 1 1 1 1))))
    (i32.add (i32.add (i32x4.extract_lane 0 (local.get 3)) (i32x4.extract_lane 1 (local.get 3)))
      (i32x4.extract_lane 2 (local.get 3)))))
