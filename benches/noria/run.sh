#!/usr/bin/bash

commit="0e1d9e2641ec11ae4fdc3f6bed8da94879d8c917"

# get the right version of noria
if [[ ! -d checkout ]]; then
	git clone https://github.com/mit-pdos/noria.git checkout
fi
git -C checkout fetch origin
git -C checkout checkout "$commit"

# patch to use local version of tokio
if grep -E '^tokio =' checkout/Cargo.toml > /dev/null; then
	# in case there's already an override in Cargo.toml
	sed -i '/^tokio /d' checkout/Cargo.toml
fi
if ! grep 'patch.crates-io' checkout/Cargo.toml > /dev/null; then
	# in case there are _no_ overrides in Cargo.toml
	echo '[patch.crates-io]' >> checkout/Cargo.toml
fi
sed -i '/patch.crates-io/a tokio = { path = "../../../tokio/" }' checkout/Cargo.toml

# time to build! sit back and relax
cd checkout
cargo build --release --bin vote

# let's run some benchmarks
for shards in 0 4; do
	for load in 1000000 2000000 3000000 4000000 5000000; do
		if [[ $shards -eq 0 ]]; then
			name="noria-unsharded-$load"
		else
			name="noria-s${shards}-$load"
		fi
		echo "run --target $load --shards $shards"
		cargo run --release --bin vote -- --warmup 10 --runtime 20 --target $load -d skewed localsoup --shards $shards > ../$name.log 2> ../$name.err
		if [[ $? -ne 0 ]]; then
			break
		fi
		if grep 'clients are falling behind' ../$name.err > /dev/null; then
			break;
		fi
	done
done
