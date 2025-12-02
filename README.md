newt stands for new terminal

commands that work
h/l for moving between cells
i for editing cell
rust/go/cpp/py/c for types of language cells
enter for running cell
:ra for running all cells
nvim tree:
space e
space h/l


the goal of newt is to write an alternative approach to using the computer than basic terminals
so essentially think about how jupyter notebooks had a benefificl impacts to how we approach computing
for example say the user wants to do a task they would typically do in a terminal
lets take the example of having an open source project where everytime what you do is start is
cd frontend
npm run build
cd ..
cargo run 
so in this case you are doing the same thing consistently 
however this is simple enough that it is not worth making another script for it
so alternatively we can implement somethign like jupyter notebooks
there you have one cell where you can keep:
cd ~/foo/frontend
npm run build
then one cell where you can keep:
cd ~/foo
cargo run
then you can easily run these cells whenever you want
and you can run them in different order
outside of normal terminal commands i also want to be able to creat cells in other languages
so for example a cell like
```rust
const foo: &str = "bar";
println!("{}", foo);
```
the basic idea is cross language notebooks
which behave like jupyter notebook in that cells can be run independently of each other
implement this new terminal system which is capable of running whatever languges are installed on the computer
write it in rust
