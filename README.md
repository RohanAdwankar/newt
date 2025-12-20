<div align="center">
 <img width="600" height="160" class="center" alt="newt" src="https://github.com/user-attachments/assets/4c881e5a-6350-4a46-9ff0-e9793b5d1a2f"/>
<table>
  <tr>
    <td width="50%">
        <video src="https://github.com/user-attachments/assets/746afae6-2823-4bbd-92b9-4f970b31ebcd"></video>
    </td>
    <td width="50%">
        <video src="https://github.com/user-attachments/assets/57b53b02-eb1d-4c13-a46c-121ee3946b71"></video>
    </td>
  </tr>
</table>
</div>


Newt offers a TUI and web GUI for a cross language jupyter notebook like experience.
The TUI can run cells in a variety of languages including rust, go, c++, python, and javscript while navigating the notebook in efficient vim-esque motions.
The GUI is a static site which allows you to run supported languages client side in WASM (js,ts,py,c++) as well as an option to connect to a local server for the remaining languages.

## Web GUI

### Client Side Mode
The goal of client side mode is to run code directly in the browser using WASM without sending anything to external servers. It currently supports python, c++, and javascript/typescript. It also supports storing files and preferences in localstorage in your browser.
To use the static site go to:

https://rohanadwankar.github.io/newt/

### Server Side Mode
The goal of server side mode is for users who would like to use the GUI but need the features of the local server (ie full language support). When the server is connected you can also interact with newt notebooks saved to applications files.
To install the server via cargo:
```bash
cargo install newts
```
To start the server run:
```bash
newts --serve
```
Then go to the main site and enable "Local network access" in your browser settings to the left of the url, and choose `Server` mode in the toolbar.

## TUI
To install the TUI via cargo:
```bash
cargo install newts
```
Then you can run it with:
```bash
newts
```

### Customization
- External Editor: Set your preferred external editor (e.g., `vim`, `nano`, `code`) using `:editor <command>`. You can also pass the editor program directly such as `:editor ~/nvim-macos-arm64/bin/nvim`
- Accent Color: Change the UI accent color using `:color <index>`, where `<index>` is a number from 0-255 representing a color from the 256-color palette. For example, `:color 40` sets it to green. To view the color options check the [ratatui docs.](https://ratatui.rs/examples/style/colors/)
 
## Keybindings & Actions

| Action | Vim Mode | Standard Mode |
| :--- | :---: | :---: |
| **Navigation** | | |
| Move Selection Down | `j` | `Arrow Down` |
| Move Selection Up | `k` | `Arrow Up` |
| Focus Sidebar | `h` | `Arrow Left` |
| Focus Editor | `l` | `Arrow Right` |
| Toggle Sidebar | `Space e` | Toolbar |
| **Cell Operations** | | |
| Edit Cell | `i` | `Enter` |
| Exit Edit Mode | `Esc` | `Esc` |
| Run Cell | `Enter` | `Shift+Enter` |
| Add Cell Below | `o` | Toolbar |
| Add Cell Above | `O` | - |
| Delete Cell | `d` | - |
| Copy Cell | `y` | - |
| Cut Cell | - | Toolbar |
| Paste Cell Below | `p` | Toolbar |
| Paste Cell Above | `P` | - |
| Toggle Fullscreen | `f` | Toolbar |
| Polling Mode | `r` | Toolbar or Right Click |
| **Sidebar Operations** | | |
| Rename File | `r` | Right Click |
| Copy File | `y` | Right Click |
| Paste File | `p` | Right Click |
| Delete File | `d` | Right Click |
| **Commands** | | |
| Run All Cells | `:ra` | Run All Button |
| Export Notebook | `:export` | Toolbar |
| Save Notebook | `:w` | Save Button |
| Quit / Close | `:q` | - |
| Change Language | `:rust`, `:py`, `:ts`, `:js`, `:cpp`, `:c` | Toolbar |
| Set Editor | `:editor <cmd>` | - |
| Set Accent Color | `:color <index>` | - |

To edit a cell in vim mode in the GUI, first press `i` once to enter the context of the cell, then use the normal vim motions to naviagate the cell, then press `i` again to enter insert mode and make changes, then press `Esc` to exit insert mode, then press `Esc` again to exit the cell context.

The TUI defaults to using the oneline editor for shell commands but to open it in your default editor the `f` key can be used. 

### Vision
The names stands for $$\textsf{{\color{green}new} {\color{green}t}erminal}$$ and the broader goal of newt is to explore an alternative approach to using the computer than basic terminals.

The core form factor is a jupyter notebook like experience with vim motions but the goal is to creatively analyze how we interact with terminals and develop a better developer experience. 
Here is a running list of usecases I am targetting.

#### Declarative Developer Environments

For example when developing [oxdraw](https://github.com/RohanAdwankar/oxdraw), because there is a Next.js site and a Rust program serving it, I was running this same sequence for every change I made:
```
cd frontend
npm run build
cd ..
cargo run 
```
This is annoyingly repetitive, however it was simple enough that it was not worth making another script for it.
But when using a newt notebook you can have a cell where you have this: 
```
cd ~/foo/frontend
npm run build
then one cell where you can keep:
cd ~/foo
cargo run
```
Then perhaps a cell with your editor:
```
vi .
```
So the full developer environment is declaritively defined in cells and the developer doesn't have to worry about making scripts for everything because recurring actions will already be in the previous cells.

#### Experimentation

Often times when developing applications my approach is to first mock up the APIs I am working with by writing curls and the jq commands to parse out what I need.
Then after I figure out how the APIs work I move them into the actual application code.
This appraoch is very inefficient because then when 2 weeks later I want to understand the APIs I do not have that terminal session. Even if I saved it to a text file it is littered in junk from bad API calls. 
Also when I parse with jq I replicate the curl which means excessive requests.
With newt its easy to write the curls or even write snippets of code in other languages and have these notes saved in your developer environment.
To improve this usecase a new feature could be a tool in the output section for JSON formatting and parsing.

#### Learning New Languages

When I was learning python the fastest way for me to get up to speed was to use the REPL because learning is through repition and having an easy way to incremently and quick fail and correct your understanding of the syntax should improve one's understanding.

#### Cloud

Google Colab is a fantastic service because it enables you to easily swap between runtimes (eg. different CPUs/GPUs). However, for obvious reasons it will not allow using alternative runtimes from different clouds.
Therefore one of the eventual visions is for each cell to be able to specify the cloud runtime.
This makes it easy to experiment with different cloud providers and hardware, as well as use the cheapest runtime available to you.
Currently the possible runtime options are client side (WASM) and server side (local server).
Adding a cloud option will require a new hosted version of the site with auth which should be a cheaper option for people who need GPU accelerated notebooks as costs and free tiers can be arbitraged across clouds.
If this interests you feel free to drop your email [here](https://forms.gle/RBgxjkPhrQ4NTbit7). 
