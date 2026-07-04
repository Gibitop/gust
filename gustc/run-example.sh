source=../examples/$1.gust
if [ -d ../examples/$1 ]; then
    source=../examples/$1/main.gust
fi

mkdir -p ../examples/build/$1
cargo run "$source" --emit-c ../examples/build/$1/$1.c -o ../examples/build/$1/$1 && ../examples/build/$1/$1

if [ ! -s ../examples/build/$1/$1 ]; then
    rm -rf ../examples/build/$1
fi
