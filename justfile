bt := '0'
export RUST_BACKTRACE := bt
default: test

run *ARGS:
  cargo run {{ARGS}}

test:
  cargo build
  cargo nextest run --all

itest:
  cargo build
  cargo nextest run --all --test '*'

snap:
  cargo build
  cargo insta test --review --all

isnap:
  cargo build
  cargo insta test --all --review --test '*'

regression:
  ./scripts/old_binary.sh
  ./scripts/bench_regression.sh ./old ./new
  rm ./old ./new

bench *ARGS:
  cargo build --release
  hyperfine "./target/release/decorous {{ARGS}}" --warmup 5
  @echo Cleaning up!
  @rm out*
  @if [[ -d "./out" ]]; then rm -rf out; fi

microbench:
  cargo bench --all
