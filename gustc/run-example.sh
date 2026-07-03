mkdir -p ../examples/build/$1
cargo run ../examples/$1.gust --emit-c ../examples/build/$1/$1.c -o ../examples/build/$1/$1 && ../examples/build/$1/$1
