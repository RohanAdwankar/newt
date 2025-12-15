newt - a new terminal
https://rohanadwankar.github.io/newt/

<add video demo here>

This repo is composed of a terminal inferface and a web GUI for a cross language jupyter notebook like experience.
The TUI features allows you to create and run cells in a variety of languages including rust, go, cpp, python, and c with the fully interface having vim-like motions.
The GUI is a static site which allows you to run some supported languages client side in WASM (js,ts,py,c++) as well as an option to connect to a server for the remaining languages.
The Server is an option for the terminal or the GUI in server mode which operates the core code execution kernel.

## Web GUI

### Client Side Mode
The goal of client side mode is to run code directly in the browser using WASM.
This means nothing is sent anywhere externally nor to a server running on your machine, just ran in the browser.
Currently supported languages are: js, ts, py, cpp

### Server Side Mode
To enable server side mode activate the server and connect to it from the GUI.
To use the main site you will have to enable "Local network access" in your browser settings to the left of the url.

## TUI
To run the TUI first start the server then start the TUI client.
The TUI will automatically start the server if it is not running.
To install the TUI via cargo:
```bash
cargo install newts
```
Then you can run it with:
```bash
newts
```
 
## Vim Motions
Currently the TUI supports the following vim-like motions: 
```
h/l for moving between cells
i for editing cell
rust/go/cpp/py/c for changint the language types of cells
enter for running cell
:ra for running all cells
y/p/P
:export
enter to open images in output cells
```
There also in a file tree with these supported motions:
```
space e for opening/closing file tree
h/l for switching from file tree to notebook
r for renaming files
y/p for file copying
```
TODO: support all motions in the gui

### Vision
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
