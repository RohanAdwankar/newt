<div align="center">
 <img width="600" height="160" class="center" alt="newt" src="https://github.com/user-attachments/assets/4c881e5a-6350-4a46-9ff0-e9793b5d1a2f"/>
 <p>newt - a new terminal</p>
</div>

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



This repo is composed of a terminal inferface and a web GUI for a cross language jupyter notebook like experience.
The TUI features allows you to create and run cells in a variety of languages including rust, go, cpp, python, and c with the fully interface having vim-like motions.
The GUI is a static site which allows you to run some supported languages client side in WASM (js,ts,py,c++) as well as an option to connect to a server for the remaining languages.
The Server is an option for the terminal or the GUI in server mode which operates the core code execution kernel.

## Web GUI

### Client Side Mode
The goal of client side mode is to run code directly in the browser using WASM.
This means nothing is sent anywhere externally nor to a server running on your machine, just ran in the browser.
Currently supported languages are: js, ts, py, cpp
Files and preferences are saved to localstorage.

### Server Side Mode
To enable server side mode activate the server and connect to it from the GUI.
To use the main site you will have to enable "Local network access" in your browser settings to the left of the url.
When the server is connected you can also interact with newt notebooks saved to applications files.
To install the server via cargo:
```bash
cargo install newts
```
To start the server run:
```bash
newts --serve
```

## TUI
To run the TUI first start the server then start the TUI client.
Notebooks are saved to applications files.
The TUI will automatically start the server if it is not running.
To install the TUI via cargo:
```bash
cargo install newts
```
Then you can run it with:
```bash
newts
```

### Customization
You can customize the TUI experience using the following commands:
- External Editor: Set your preferred external editor (e.g., `vim`, `nano`, `code`) using `:editor <command>`. This setting is persistent. You can also pass the editor program directly such as `:editor ~/nvim-macos-arm64/bin/nvim`
- Accent Color: Change the UI accent color using `:color <index>`, where `<index>` is a number from 0-255 representing a color from the 256-color palette. For example, `:color 40` sets it to green. To view the color options check the [ratatui docs](https://ratatui.rs/examples/style/colors/)
 
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

The TUI defaults to using the oneline editor for shell commands but to open it in your default editor the f key can be used. 

### Vision
The goal of newt is to explore an alternative approach to using the computer than basic terminals.

The core form factor is a jupyter notebook like experience with vim motions but the goal is to creatively analyze how we interact with terminals and develop a better develop experience. 
Here is a running list of usecases I am targetting.

#### Declarative Developer Environments

For example say the user wants to do a task they would typically do in a terminal:
```
cd frontend
npm run build
cd ..
cargo run 
```
So in this case you are typing the same thing consistently.
However this is simple enough that it is not worth making another script for it
So when using a newt notebook you can have a cell where you have this: 
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
So the full developer environment is declaritively defined in cells.

#### Experimentation

Often times when developing applications my approach is to first mock up the APIs I am working with by writing curls and the jq commands to parse out what I need.
Then after I figure out how the APIs work I move them into the actual application code.
This appraoch is very inefficient because then when 2 weeks later I want to understand the APIs I do not have that terminal session. Even if I saved it to a text file it is littered in junk from bad API calls. 
Also when I parse with jq I replicate the curl which means excessive requests.
With newt its easy to write the curls or even write snippets of code in other languages and have these notes saved in your developer environment.
To improve this a new feature could be a tool in the output section for JSON formatting and parsing.

#### Learning New Languages

When I was learning python the fastest way for me to get up to speed was to use the REPL because learning is through repition and having an easy way to incremently and quick fail and correct your understanding of the syntax should improve one's understanding.

#### Cloud

Google Colab is a fantastic service because it enables you to easily swap between runtimes (eg. different GPUs/TPUs). However, for obvious reasons it will not allow using alternative runtimes from different clouds.
Therefore one of the eventual visions is for each cell to be able to specify the cloud runtime.
This makes it easy to experiment with different cloud providers and hardware, as well as use the cheapest runtime available to you.
Currently the possible runtime options are client side (WASM) and server side (local server).
Adding a cloud option will require a new hosted version of the site with auth and billing which should be a cheaper option for people who need GPU accelerated notebooks.
