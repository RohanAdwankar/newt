<div align="center">
 <img width="600" height="160" class="center" alt="newt" src="https://github.com/user-attachments/assets/4c881e5a-6350-4a46-9ff0-e9793b5d1a2f"/>
<table>
  <tr>
    <td width="33%">
        <video src="https://github.com/user-attachments/assets/f7339520-e4d4-48da-82a3-19344926215b"></video>
    </td>
    <td width="33%">
        <video src="https://github.com/user-attachments/assets/746afae6-2823-4bbd-92b9-4f970b31ebcd"></video>
    </td>
    <td width="33%">
        <video src="https://github.com/user-attachments/assets/57b53b02-eb1d-4c13-a46c-121ee3946b71"></video>
    </td>
  </tr>
</table>
</div>

Newt is a new sort of terminal. It embraces a cross language notebook experience with a TUI and GUI option.
The TUI can run cells in a variety of languages including rust, go, c++, python, and javscript while navigating the notebook in efficient vim-esque motions.
The GUI is a static site which allows you to run supported languages in the browser (js,ts,py,c++) as well as an option to connect to your local computer.

## Web GUI

### Client Side Mode
The goal of client side mode is to run code directly in the browser using WASM without sending anything to external servers. It currently supports python, c++, and javascript/typescript. It also supports storing files and preferences in localstorage in your browser.
To use the static site go to:
```bash
open https://rohanadwankar.github.io/newt/
```
### Server Side Mode
The goal of server side mode is for users who would like to use the GUI but need the features of the local server (eg. full language support, local file system).
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

### External Editor 
To edit files that aren't .md/.newt files in the TUI or edit cells that are code cells the TUI uses your preferred external editor.
Set your preferred external editor (e.g., `vim`, `nano`, `code`) using `:editor <command>`.
You can also pass the editor program directly such as `:editor ~/nvim-macos-arm64/bin/nvim`
 
## CLI Flags
| Action | Description | 
| :--- |  :---: |
| newts file-path | Opens file at file-path. |
| newts --serve | Starts the newt server allowing the GUI to work in server mode. |
| newts --term file-path | Converts the terminal output from saving a terminal session to a file into markdown. |
| newts run <file> <heading> | Runs the codeblocks in a file or under a markdown heading. |

## Keybindings & Actions

| Action | Vim Mode | Standard Mode |
| :--- | :---: | :---: |
| **Navigation** | | |
| Move Selection Down | `j` | `Arrow Down` |
| Move Selection Up | `k` | `Arrow Up` |
| Focus Sidebar | `space h` | `Arrow Left` |
| Focus Editor | `space l` | `Arrow Right` |
| Toggle Sidebar | `Space e` | Toolbar |
| Search | `/` | - |
| Move to Next/Previous Search Match | `n/N` | - |
| Jump to Top | `gg` | Toolbar |
| Jump to Bottom | `G` | Toolbar |
| Jump to Cell 3 | `:3` or `3G` | Toolbar |
| Move 3 Cells Down | `3j` | |
| **Cell Operations** | | |
| Edit Cell | `i` | `Enter` |
| Open Output Cell In Editor | `I` | - |
| Exit Edit Mode | `Esc` | `Esc` |
| Run Cell | `Enter` | `Shift+Enter` |
| Add Cell Below | `o` | Toolbar |
| Add Cell Above | `O` | - |
| Delete Cell | `d` | - |
| Copy Input Cell | `y` | - |
| Copy Output Cell | `Y` | - |
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
| Open Folder | `l` | - |
| Close Folder | `h` | - |
| Toggle Folder | `enter` | - |
| Move 3 Files Down | `3j` | |
| Jump to Top (Create New Notebook) | `gg` | |
| Jump to Bottom | `G` | |
| **Commands** | | |
| Run All Cells | `:ra` | Run All Button |
| Export Notebook | `:export` | Toolbar |
| Save Notebook | `:w` | Save Button |
| Save Snapshot to `.newt/` | `:ws` | - |
| Quit / Close | `:q` | - |
| Change Language | `:rust`, `:py`, `:ts`, `:js`, `:cpp`, `:c` | Toolbar |
| Set Editor | `:editor <cmd>` | - |
| Set Accent Color | `:color <index>` | - |

To edit a cell in vim mode in the GUI, first press `i` once to enter the context of the cell, then use the normal vim motions to naviagate the cell, then press `i` again to enter insert mode and make changes, then press `Esc` to exit insert mode, then press `Esc` again to exit the cell context.

The TUI defaults to using the oneline editor for shell commands but to open it in your default editor the `f` key can be used. 

### Vision
The names stands for $$\textsf{{\color{green}new} {\color{green}t}erminal}$$ and the broader goal of newt is to creatively analyze how we interact with terminals and develop a better developer experience. 
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

#### Documenation, Education, and AI 

Since newt saves files as markdown it is easy to read for documentaion but also the instructions can be easily followed along with. 
Further, newt has an export option in the GUI which allows you to create links which define the notebook contents.
Therefore they are easy to share and run.
For similar reasons to why they would be easy to use to educate humans, its helpful for providing context to LLMs which as seen in things like Claude Skills prefer markdown.

#### Runbooks

Often times on-call / SRE teams are solving probles much faster than can be documented for later use.
Newt aims to be a self documenting terminal meaning that if someone solves the problem in newt then that file itself acts as readable documentation on how to solve the problem.

#### Experimentation

Often times when developing applications my approach is to first mock up the APIs I am working with by writing curls and the jq commands to parse out what I need.
This appraoch is very inefficient because then when 2 weeks later I want to understand the APIs I do not have that terminal session. 
Even if I saved it to a text file or a tmux session it is littered in junk from bad API calls.
In contrast the notebook appraoch makes this issue of irrelevant commands in the file void because in a notebook if a curl is wrong you just edit that cell while the default appraoch in a traditional terminal is to click up to get the curl again and then edit it.
With newt its easy to write the curls or even write snippets of code in other languages and have these notes saved in your developer environment.

#### Move Behaviors away from CLI 

I often do a lot of things with CLIs (curl,jq,awk,sed etc) just because I dont want to do the various setup steps neccesary to start using the language my codebase is actually written in.
This is fast at the start but ultimately the commands have to be translated over and time is being wasted writing the logic twice.
Newt aims to decrease the friction of using your codebase's language isntead of CLIs since is right in the terminal and is using your repo's environemnt and package manager.  

#### Cloud

Google Colab is a fantastic service because it enables you to easily swap between runtimes (eg. different CPUs/GPUs). However, it does not allow using alternative runtimes from different clouds.
Therefore one of the eventual visions is for each cell to be able to specify the cloud runtime.
This makes it easy to experiment with different cloud providers and hardware, as well as use the cheapest runtime available to you.
