(module
  (import "env" "memory" (memory 0))
  (import "unnamed-builtin" "sqrt" (func $sqrt (param f32) (result f32)))
  (import "builtin" "floor" (func $floor (param f32) (result f32)))
  ;; sum of 0..n via loop + br_if
  (func (export "sum") (param $n f32) (result f32)
    (local $i f32) (local $acc f32)
    (block $done
      (loop $top
        (br_if $done (f32.ge (local.get $i) (local.get $n)))
        (local.set $acc (f32.add (local.get $acc) (local.get $i)))
        (local.set $i (f32.add (local.get $i) (f32.const 1)))
        (br $top)))
    (local.get $acc))
  ;; if/else with a result, calling a builtin
  (func (export "pick") (param $x f32) (result f32)
    (if (result f32) (f32.lt (local.get $x) (f32.const 0))
      (then (f32.neg (local.get $x)))
      (else (call $floor (call $sqrt (local.get $x))))))
  ;; br_table
  (func (export "classify") (param $x i32) (result f32)
    (block $c
      (block $b
        (block $a
          (br_table $a $b $c (local.get $x)))
        (return (f32.const 10)))
      (return (f32.const 20)))
    (f32.const 30)))
