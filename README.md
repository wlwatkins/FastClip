# FastClip

<p align="center">
  <img src="src-tauri/icons/Square310x310Logo.png" width="100" alt="App Icon">
</p>

FastClip is a Windows desktop application built with [Tauri](https://tauri.app/) that allows users to create and manage macro buttons for quickly copying predefined text snippets to the clipboard.

## Security

**By default, clips are stored unencrypted on this computer.** FastClip has an
optional PIN protection setting that encrypts the store, but it is off until
you turn it on. If you never open Settings, your clips are plaintext.

Turning PIN protection on does not make FastClip a password manager, and it
does not protect against another program running under your Windows account —
that program can read the same files FastClip can. What it does protect is the
file leaving this machine: cloud sync, backup images, a shared or resold
computer, a disk without BitLocker.

**If you forget the PIN, your clips cannot be recovered.** There is no reset
and no backdoor. Export a copy before you turn PIN protection on if you want a
fallback.

## Overview

FastClip is designed to streamline repetitive text-copying tasks by allowing users to create customisable macro buttons. Each button is assigned a predefined text snippet, which is copied to the clipboard with a single click. This tool is ideal for developers, customer support agents, and anyone who frequently needs to paste standard text responses or code snippets.

Key benefits of FastClip:

- Saves time by reducing manual copying and pasting.
- Simple and intuitive UI for managing macros.
- Lightweight and fast, powered by Tauri for minimal resource usage.
- Optional PIN protection for the local store — off by default, see [Security](#security).
- Cross-platform potential with a focus on Windows.
- With FastClip, you can optimise your workflow and increase productivity effortlessly.

![Screenshot](assets/images/screenshot.png)

## Features

- Create customisable macro buttons
- Store predefined text snippets
- Click a button to instantly copy the text to the clipboard
- Choose the colour of your button
- Keep window always on top
- Drag to reorder, and search by label or value
- Export and import clips as JSON
- Optional PIN protection, gated by Windows DPAPI as well as the PIN — see [Security](#security)

## Installation

To install FastClip, download the NSIS executable and follow the installation instructions.

## Usage

![Demonstration](assets/images/demonstration.gif)
## Configuration

Settings currently cover whether the app stays on top of other windows, and
PIN protection for the store (see [Security](#security)).

## Building from Source

The project is built with Tauri and Svelte.

### Prerequisites

- Rust & Cargo installed
- Node.js & npm installed

### Steps

1. Clone this repository:
   ```sh
   git clone <repo_url>
   cd <repo_name>
   ```
2. Install dependencies:
   ```sh
   npm install
   ```
3. Build and run the application:
   ```sh
   npm run tauri dev
   ```

## Todo

- [ ] Improve the UI with better animations.
- [ ] Add file copy functionality.
- [ ] Implement keyboard shortcuts for quick macro activation.
- [ ] Support multiple clipboard entries with history.
- [ ] Cloud synchronization for macros across devices.


## Contributing

This is an early proof of concept. I think the app could benefit from nice CSS animations to make it more intuitive.
Maybe add more features. If I ever get traction, perhaps we can improve this.
If you want to help out, we will organise the contribution if I ever receive some feedback.

## Licence

 <p xmlns:cc="http://creativecommons.org/ns#" xmlns:dct="http://purl.org/dc/terms/"><span property="dct:title">FastClip</span> by <span property="cc:attributionName">wlwatkins</span> is licensed under <a href="https://creativecommons.org/licenses/by/4.0/?ref=chooser-v1" target="_blank" rel="license noopener noreferrer" style="display:inline-block;">CC BY 4.0<img style="height:22px!important;margin-left:3px;vertical-align:text-bottom;" src="https://mirrors.creativecommons.org/presskit/icons/cc.svg?ref=chooser-v1" alt=""><img style="height:22px!important;margin-left:3px;vertical-align:text-bottom;" src="https://mirrors.creativecommons.org/presskit/icons/by.svg?ref=chooser-v1" alt=""></a></p> 