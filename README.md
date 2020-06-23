# ReaLearn

__This README is just a draft! Will be improved soon.__

## Build

On Linux we need PHP in order to translate the dialog resource file to C++. For EEL on Linux, we also need nasm. For
clipboard access on Linux, we need xorg-dev and related stuff (changed to more polished `clipboard` crate instead of 
doing our own implementation via SWELL in commit `c3e28e92b75`).

```sh
sudo apt install php nasm xorg-dev libxcb-shape0-dev libxcb-render0-dev libxcb-xfixes0-dev -y
```

```
cargo install --force cargo-make
cargo make dist
```

`rustup default nightly-2020-05-15`

## Design

### Differences between model data, model and final object

By example of the concept of a _Target_:

- `TargetModelData`
    - Used for serialization/deserialization of ReaLearn's session state
    - Optimized for being represented as JSON
    - Plain data structure, completely stand-alone, free from any *reaper-rs* runtime-only data types
    - Easy to construct (doesn't need any REAPER functions or session context)
    - Can be applied to models
- `TargetModel`
    - Used as UI model for "building" targets
    - Contains reactive properties which the UI can subscribe to for getting notified about value changes
    - Properties exist for all possible target types so settings are memorized between type switches
    - Also contains *reaper-rs* runtime-only data types
    - Has no awareness of the session context, which is the context in which it is used 
      (containing ReaLearn FX instance, project, etc.).
    - In order to construct the model from `TargetModelData`, it may be necessary to execute REAPER functions.
      And in some cases it also needs the session context (e.g. for looking up a particular track by name). 
- `Target`
    - Used for the actual control/feedback processing
    - Light-weight (contains just the data necessary for the specific target type)
    - Immutable, easy to clone
    - Completely resolved (e.g. no virtual tracks anymore)
    
A target is special in that it also has a 4th type:

- `TargetModelWithContext`
    - Used for resolving the last remaining virtual references
    - A `TargetModel` which is aware of the containing ReaLearn FX instance
    - This context is the last missing piece of information for being able to construct the final target.
      E.g. it is used to resolve the real REAPER track from a virtual _&lt;This&gt;_ track.