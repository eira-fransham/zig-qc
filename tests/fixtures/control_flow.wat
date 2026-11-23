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
    (f32.const 30))
  ;; br_table with repeated targets ($a for 0 and 2, $b for 1 and 4), a case
  ;; sharing the default ($d for 3), and arms that rejoin
  (func (export "table_join") (param $x i32) (result f32)
    (local $r f32)
    (block $join
      (block $d
        (block $b
          (block $a
            (br_table $a $b $a $d $b $d (local.get $x)))
          (local.set $r (f32.convert_i32_s (local.get $x)))
          (br $join))
        (local.set $r (f32.const 20))
        (br $join))
      (local.set $r (f32.const 30)))
    (f32.add (local.get $r) (f32.const 0.5)))
  ;; if/else where both branches rejoin, then a second if
  (func (export "two_ifs") (param $x f32) (result f32)
    (local $r f32)
    (if (f32.lt (local.get $x) (f32.const 0))
      (then (local.set $r (f32.const 1)))
      (else (local.set $r (f32.const 2))))
    (if (f32.gt (local.get $x) (f32.const 10))
      (then (local.set $r (f32.add (local.get $r) (f32.const 10)))))
    (local.get $r)))
