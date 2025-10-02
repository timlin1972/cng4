#! /bin/bash

cng4="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

while true
do
	cd "$cng4"
	
	cd ..
	/usr/bin/git fetch --all
	/usr/bin/git reset --hard origin/main

	# cd client
	# npm install
	# npm run build
	# cd ..

	# cd server
	$HOME/.cargo/bin/cargo run
	# cd ..
done
