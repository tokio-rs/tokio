#!/usr/bin/bash
set -o nounset
set -o errexit

commit="88c4086c0e82d6f1ef427460144d2ecde7ab1725"

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
	for load in {1,2,3,4,5,6,7,8,9,10}000000; do
		if [[ $shards -eq 0 ]]; then
			name="noria-unsharded-$load"
		else
			name="noria-s${shards}-$load"
		fi
		echo "run --target $load --shards $shards"
		if ! cargo run --release --bin vote -- \
			--warmup 10 --runtime 20 --target $load -d skewed \
			localsoup --shards $shards \
			> ../$name.log 2> ../$name.err; then
			echo " -> run command failed"
			break
		fi
		if grep 'clients are falling behind' ../$name.err > /dev/null; then
			echo " -> cancelling early as server is not keeping up"
			break;
		fi
	done
done
