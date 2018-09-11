#!/bin/sh
set -ex

if [ ! -d tmp ]; then
  cargo new tmp
  cat >> tmp/Cargo.toml <<-EOF
futures = "0.1.24"
tokio = "0.1.8"

[workspace]
EOF
  cargo build --manifest-path tmp/Cargo.toml
fi

# rand=$(ls tmp/target/debug/deps/librand-*.rlib | head -1)
# echo $rand

for f in $(git ls-files | grep '.md$' | grep '^docs'); do
  echo "$f"
  cp $f tmp/guide.md
  sed -i -e 's/\<\!\-\-//g' tmp/guide.md
  sed -i -e 's/\-\-\>//g' tmp/guide.md
  rustdoc --test "tmp/guide.md" -L tmp/target/debug/deps
done
