# ReaLearn

[![Windows x86_64](https://github.com/helgoboss/realearn/actions/workflows/windows-x86_64.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/windows-x86_64.yml)
[![Windows i686](https://github.com/helgoboss/realearn/actions/workflows/windows-i686.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/windows-i686.yml)
[![macOS x86_64](https://github.com/helgoboss/realearn/actions/workflows/macos-x86_64.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/macos-x86_64.yml)
[![macOS aarch64](https://github.com/helgoboss/realearn/actions/workflows/macos-aarch64.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/macos-aarch64.yml)
[![Linux x86_64](https://github.com/helgoboss/realearn/actions/workflows/linux-x86_64.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/linux-x86_64.yml)
[![Linux aarch64](https://github.com/helgoboss/realearn/actions/workflows/linux-aarch64.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/linux-aarch64.yml)
[![Linux armv7](https://github.com/helgoboss/realearn/actions/workflows/linux-armv7.yml/badge.svg)](https://github.com/helgoboss/realearn/actions/workflows/linux-armv7.yml)
[![GitHub license](https://img.shields.io/badge/license-GPL-blue.svg)](https://raw.githubusercontent.com/helgoboss/realearn/master/LICENSE)
[![Donate](https://img.shields.io/badge/Donate-PayPal-orange.svg)](https://www.paypal.com/cgi-bin/webscr?cmd=_s-xclick&hosted_button_id=9CTAK2KKA8Z2S&source=url)

Sophisticated [REAPER](https://www.reaper.fm/) MIDI/OSC-learn VST instrument plug-in for controlling REAPER with feedback.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
- [Contributing](#contributing)
- [Architecture](#architecture)
- [Links](#links)

## Installation

The easiest and preferred way of installing ReaLearn is via [ReaPack](https://reapack.com/), a
sort of "app store" for REAPER. It allows you to keep your installation of ReaLearn up-to-date very easily.

### Install for the first time

If you previously installed ReaLearn manually, please uninstall it first!

1. Install [ReaPack](https://reapack.com/) if not done so already
2. Extensions → ReaPack → Import repositories...
3. Copy and paste the following repository URL:
   https://github.com/helgoboss/reaper-packages/raw/master/index.xml
4. Extensions → ReaPack → Browse packages...
5. Search for `realearn`
6. Right mouse click on the ReaLearn package → Install...
7. OK or Apply
8. Restart REAPER

### Update to the latest stable version

ReaLearn development moves fast. In order to take advantage of new features, improvements and fixes, you should check for updates from time to time.

1. Extensions → ReaPack → Synchronize packages
    - It will tell you if a new version has been installed.
2. Restart REAPER

### Test new features and improvements

If you want to get access to cutting-edge but untested versions of ReaLearn, you have two options:

Install a specific pre-release:

1. Right mouse click on the ReaLearn package → Versions
2. Select any version ending on `-pre.*` or `-rc.*`
3. OK or Apply
4. Restart REAPER

Enable pre-releases globally:

1. Extensions → ReaPack → Manage repositories → Options... → Enable pre-releases globally (bleeding edge)
2. After that, whenever you synchronize packages, you will get the latest stuff.

### Install manually

If you are more the download type of person, you can find the latest `dll`, `dylib` and `so` files here at
GitHub on the [releases page](https://github.com/helgoboss/realearn/releases) for manual installation.
You also must install ReaLearn manually if you plan to use ReaLearn in both REAPER for Windows 32-bit
and REAPER for Windows 64-bit because then it's important to use two separate VST plug-in directories.

**Please note that it's impossible to run ReaLearn as a bridged plug-in.** If you have
"Preferences → Plug-ins → Compatibility → VST bridging/firewalling" set to "In separate plug-in process" or
"In dedicated process per plug-in", you will need to add an exception for ReaLearn by setting "Run as" to
"Native only"!

## Usage

A complete user guide for the latest release is available as 
[PDF](https://github.com/helgoboss/realearn/releases/latest/download/realearn-user-guide.pdf) and
[HTML (website)](https://www.helgoboss.org/projects/realearn/user-guide). The user guide of the latest not-yet-released version is available as 
[HTML (GitHub)](https://github.com/helgoboss/realearn/blob/master/doc/user-guide.adoc).

We also have an [introduction video](https://www.youtube.com/watch?v=dUPyqYaIkYA). Watch 2 minutes to get a first
impression and stay tuned if you are interested in the details.

### Quick start

ReaLearn is fired up just like any other VST instrument in REAPER: By adding it to an FX chain.

**Main panel (containing the list of mappings):**

<img alt="Main panel" src="doc/images/screenshot-main-panel.png" width="600">

**Mapping panel (for editing one particular mapping):**

<img alt="Mapping panel" src="doc/images/screenshot-mapping-panel.png" width="600">

## Contributing

See [CONTRIBUTING](CONTRIBUTING.md).

## Architecture

See [ARCHITECTURE](ARCHITECTURE.md).

## Links

- [Website](https://www.helgoboss.org/projects/realearn/)
- [Forum](http://forum.cockos.com/showthread.php?t=178015) (dedicated thread in REAPER forum)
- [Issue tracker](https://github.com/helgoboss/realearn/issues)
- [Old issue tracker](https://bitbucket.org/helgoboss/realearn/issues) (for ReaLearn < v1.10.0)
- [ReaLearn Companion app](https://github.com/helgoboss/realearn-companion)