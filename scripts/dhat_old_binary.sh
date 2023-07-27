#!/bin/bash

# This script creates two versions of the decorous binary: one for the current
# (uncommited) changes on main, and one for the previous commit. They are both
# compiled with the `dhat-heap` feature (for heap profiling).

cargo build --features dhat-heap --release
mv ./target/release/decorous ./new

git stash --include-untracked
git checkout "${1:-HEAD~1}"

cargo build --features dhat-heap --release
mv ./target/release/decorous ./old

git checkout main
git stash pop
