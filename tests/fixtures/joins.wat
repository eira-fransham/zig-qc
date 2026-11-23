(module
  (import "env" "memory" (memory 0))
  (global $g (mut f32) (f32.const 0))
  ;; A join whose predecessors are all inside one branch: it follows the
  ;; inner `if` there, rather than the outer one. (`__wasm_f176` in qw-zig.)
  (func (export "join_in_branch") (param $a i32) (param $b i32) (param $c i32) (result i32)
    (local $r i32)
    (local.set $r (i32.const 1))
    (if (i32.eqz (local.get $a)) (then (return (i32.const 0))))
    (block $join
      (br_if $join (i32.eqz (local.get $b)))
      (local.set $r (i32.add (local.get $r) (i32.const 10)))
      (if (i32.gt_s (local.get $c) (i32.const 5)) (then (return (i32.const 0)))))
    (i32.mul (local.get $r) (i32.const 3)))
  ;; A shared tail that one path skips: it keeps its `goto` (duplicating the
  ;; tail would make the bytecode bigger). (`trigger_secret` in qw-zig.)
  (func (export "skip_tail") (param $x i32) (result f32)
    (local $s i32) (local $out i32)
    (block $done
      (block $tail
        (if (i32.eq (local.get $x) (i32.const 0))
          (then (local.set $s (i32.const 1)) (br $tail)))
        (if (i32.eq (local.get $x) (i32.const 1))
          (then (local.set $s (i32.const 2)) (br $tail)))
        (br_if $done (i32.ne (local.get $x) (i32.const 2)))
        (local.set $s (i32.const 3)))
      (local.set $out (i32.mul (local.get $s) (i32.const 100)))
      (global.set $g (f32.add (global.get $g) (f32.const 1))))
    (f32.add (f32.convert_i32_s (local.get $out)) (global.get $g)))
  ;; `a > 10 || (b && c < 5)`, as chained branches: these stay nested `if`s,
  ;; which fteqcc compiles to fewer instructions than `&&` and `||`.
  (func (export "and_or") (param $a i32) (param $b i32) (param $c i32) (result i32)
    (block $no
      (block $yes
        (br_if $yes (i32.gt_s (local.get $a) (i32.const 10)))
        (br_if $no (i32.eqz (local.get $b)))
        (br_if $no (i32.ge_s (local.get $c) (i32.const 5))))
      (return (i32.const 1)))
    (i32.const 0)))
