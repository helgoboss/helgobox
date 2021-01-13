<table class="table">
<tr>
  <td>Last update of text:</td>
  <td><code>2021-01-13 (v1.12.0-pre10)</code></td>
</tr>
<tr>
  <td>Last update of relevant screenshots:</td>
  <td><code>2021-01-09 (v1.12.0-pre9)</code></td>
</tr>
</table>

## Introduction

### What is ReaLearn?

Probably you know already that ReaLearn is a kind of improved MIDI learn for REAPER. But what is it
exactly? Let me put it this way:

> ReaLearn is an instrument. It allows you to take whatever MIDI controller you have, be it a
> keyboard or some fader box, plug it in and play ... but instead of playing notes, you "play"
> REAPER itself!
>
> And because ReaLearn supports MIDI feedback, you can also do the reverse: Let REAPER "play" your
> controller.

While this is still pretty vague, I think it captures the essence of ReaLearn. From a technical
viewpoint it's a VSTi plug-in, so it is an instrument, quite literally. That's one thing that sets
it immediately apart from the more conventional control surface feature in REAPER and 3rd-party
efforts such as [CSI](https://forum.cockos.com/showthread.php?t=183143) or
[DrivenByMoss](http://www.mossgrabers.de/Software/Reaper/Reaper.html). The goal of the
latter-mentioned is to equip REAPER with out-of-the-box support for specific controllers, typically
dedicated DAW controllers such as
[Mackie MCU](https://upload.wikimedia.org/wikipedia/commons/thumb/e/e5/Mackie_Control_Universal.jpg/1600px-Mackie_Control_Universal.jpg)
that are tailored to control a DAW just like a hardware mixer. And I suppose they do a pretty good
job at that.

ReaLearn's approach is quite different: It gives you total control on which control element operates which REAPER
parameter and provides you with a *learn* function which allows you build your own control mappings quickly
and intuitively without writing configuration files. All of that on a *per-instance* basis. That's right, the mappings
are saved as part of the ReaLearn instance and therefore as part of your REAPER project. No need to pollute your global
control mappings just for the needs of one project!

Nevertheless, since version 1.12.0, ReaLearn is also a great choice for setting up global mappings for usage across 
multiple projects. It provides a simple yet powerful preset system to make a set of mappings reusable in all of your
projects. Just add ReaLearn to the monitoring FX chain of REAPER (View → Monitoring FX) and ReaLearn will be instantly
available in all of your REAPER sessions without having to add it to a project first.

ReaLearn is designed to get the most out of general-purpose MIDI controllers, which - compared to the big
and bulky DAW consoles - usually have the advantage of being small, USB-powered, more versatile and easier on the
budget. ReaLearn doesn't impose many requirements on your controller. Thanks to features like conditional
activation and projection, it can turn even the cheapest MIDI controller into a powerhouse for controlling
your DAW.  

The usual ReaLearn workflow for a single mapping goes like this:

1. Add a mapping
2. Hit "Learn source" and touch some knob on your controller.
3. Hit "Learn target" and touch some target parameter.
4. Done.

If you want to learn multiple mappings in one go, this gets even easier via the "Learn many" button which will save you
*a lot of* clicks.

The result are mappings that you can customize as you desire, for example by setting a target value
range. All of that with MIDI feedback support, which was previously only available in the less
dynamic, more global control surface world.

**Summary:** _ReaLearn is a sort of instrument for controlling REAPER._

### Usage scenarios

Ultimately, ReaLearn gains whatever purpose you can come up with. Because it is a VSTi plug-in and
provides many MIDI routing options, it's very flexible in how it can be used. You can "inject" it
wherever you want or need it (limitation: using it in a take FX chain is not possible yet):

- **Input FX chain for live-only use:** Put it on a track's input FX chain in order to use it only
  for incoming "live" MIDI and let it control a parameter of an effect that's on the normal FX
  chain, right below a synthesizer. It will be active only if the track is armed for recording.
  All MIDI messages that are used for parameter control will *automatically* be filtered by default
  and won't reach the controlled instrument, which is usually exactly what you need.
- **Grid controller for song switching:** Use some grid controller like the
  [AKAI APC Key 25](https://thumbs.static-thomann.de/thumb/thumb250x220/pics/prod/339386.jpg) to
  arm/disarm various tracks (effectively enabling/disabling certain sound setups) by pressing the
  grid buttons - with the LEDs of the buttons indicating which setup is currently active.
- **Combination with other MIDI FX for interesting effects:** Slap it on a track FX chain, right
  between a MIDI arpeggiator and a synthesizer to arpeggiate the cutoff parameter of that
  synthesizer.
- **Monitoring FX for project-spanning setups:** Put it on the monitoring FX chain to have some
  control mappings available globally in all projects (similar to conventional control surface
  stuff).
- **Unusual settings for experimental stuff:** Create a track volume mapping with only feedback
  turned on. Choose "&lt;FX output&gt;" as MIDI feedback output and play the synthesizer one
  position below in the FX chain by moving the track volume slider (whatever that might be good for
  ...).
- **Rotary encoders for avoiding parameter jumps:** How about a refreshingly "normal" use case? Let
  your rotary endless encoder control a track send volume without parameter jumps and restrict the
  value range to volumes below 0dB.
- **VST presets for easy reuse:** Save a bunch of commonly used mappings globally as FX presets.
- **Switching controller and main presets separately:** Maintain controller and main presets and switch
  between them as you like. Easily switch your controller without adjusting your FX presets.
- **Combination of multiple instances:** Use one ReaLearn instance to arm or disarm tracks that
  contain other ReaLearn instances to enable/disable different mapping groups. Group mappings and
  activate/deactivate them group-wise simply by instantiating multiple ReaLearn instances and
  enabling/disabling them as desired in the FX chain window.

... the possibilities are endless. It's all up to you! Use your creativity.

All of that makes ReaLearn especially well-suited for performers, people who use REAPER as a
platform for live playing. It might be less interesting to people who use REAPER for arranging,
mixing and mastering only and are satisfied with a control surface off the shelf. But even so,
as long as you have some general-purpose MIDI controller and you want a fine-tuned mapping to DAW parameters
of all sorts, give ReaLearn a try. It might be just what you need. More so if the controller supports feedback
(e.g. motorized faders or LEDs).

**Summary:** _ReaLearn is tailored to usage scenarios typically desired by performers._

### Videos

If you want to get a first impression of ReaLearn, a video is surely a good way. Here a short, non-complete list:

- [How To: ReaLearn and MIDI Controller for Track Sends in REAPER - Tutorial](https://www.youtube.com/watch?v=WKF2LmIueY8)
- [using ReaLearn to assign MIDI controllers to (VST) plugin parameters in Cockos Reaper](https://www.youtube.com/watch?v=UrYrAxnB19I)
- [MIDI Controller Feedback in REAPER with ReaLearn and LBX SmartKnobs - Tutorial](https://www.youtube.com/watch?v=p0LBdXXcg7g)
- [Demonstration of the projection feature](https://www.youtube.com/watch?v=omuYBznEShk)

Keep in mind that some of the mentioned videos still use older versions of ReaLearn. E.g. it should be easier 
nowadays to use ReaLearn's "preset auto-load" feature instead of adding LBX SmartKnobs to the mix.

## Basics

### Control

After installing ReaLearn, you can fire it up just like any other VST instrument in REAPER: By
adding it to an FX chain.

1. Right click in the track header area and choose "Insert virtual instrument on new track..."
2. Choose "VSTi: ReaLearn (Helgoboss)"

After that you should see ReaLearn's main panel (unlike this screenshot, it wouldn't contain any
mappings yet):

![Main panel](images/screenshot-main-panel-annotated.svg)

On the very top you see the _header panel_ for changing settings or doing things that affect
this complete instance of ReaLearn. Below that there's the _mapping rows panel_ which displays all
main mappings in this instance of ReaLearn. There can be very many of them. I've heard from users who use
hundreds. On the very bottom you see some information about the version of ReaLearn that you are
running. It's important to include this information in bug reports.

#### Adding a mapping

**Let's see how to add and use our first mapping:**

1. Press the "Add one" button.
   - A new mapping called "1" should appear in the mapping rows panel.
   - For now it's greyed out because it's not complete yet. The default target is a *Track FX parameter* target
     which doesn't yet refer to any specific FX. 
2. Press the "Learn source" button of that new mapping.
   - Its label will change to "Stop".
3. Touch some control element on your MIDI controller (knob, encoder, fader, button, key, pitch
   bend, mod wheel, ...). For this example it's best to use something continuous, not a button or
   key.
   - If your MIDI is set up correctly, the button label should jump back to "Learn source" and the
     touched control element should appear in the _source label_. See below if this doesn't happen.
4. Press the "Learn target" button.
   - Its label will change to "Stop".
5. Touch the volume fader of your newly created REAPER track.
   - The button label should jump back to "Learn target" and "Track volume" should appear in the
     _target label_.
   - At this point the mapping should not be greyed out anymore because it's complete and enabled. 
6. Now you should be able to control the touched target with your control element.

#### Troubleshooting

If REAPER crashes when scanning for plug-ins and the crash message shows something like `reaper_host64`
or `reaper_host32`, you either have a 32/64-bit version mismatch or you have 
"Preferences → Plug-ins → Compatibility → VST bridging/firewalling" set to "In separate plug-in process" or
"In dedicated process per plug-in". Please see the [installation instructions on the
project website](https://github.com/helgoboss/realearn#installation) for hints how to fix this.
In future, ReaLearn hopefully will handle this situation more gracefully. 

If the label remains at "Stop" at step 3, you need to have a look at your MIDI setup.

- Make sure **Enable input from this device** is checked for your controller MIDI input device in
  the REAPER preferences.
  - Please note: _Enable input for control messages_ is totally irrelevant for ReaLearn. This is
    only used for REAPER's built-in MIDI learn, which uses the so-called _control MIDI path_.
    ReaLearn on the other hand uses the track MIDI path - which is one reason why it is so flexible.
- Make sure your audio hardware is not stuck (playback in REAPER should work).
- Make sure the track is armed for recording and has the appropriate MIDI device input.
- Make sure your controller is in MIDI mode.
   - Some controllers, especially DAW controllers, are able to work with several protocols (MCU, HUI, ...).
   - ReaLearn doesn't understand those proprietary protocols. Therefore you need to consult your controller's manual
     and take the necessary steps to put it into something like a "general-purpose MIDI" mode.
   - Example: Presonus Faderport

When you read this the first time, you might get the impression that this is a lot of work for
setting up one simple control mapping. It's not. Learning mappings is a matter of a few secs after
you got the hang of it. ReaLearn also provides the "Learn many" button and the REAPER action
"Learn source for last touched target" for further speeding up this step. More about that later.

At this point: Congratulations! You have successfully made your first baby steps with ReaLearn.

#### Some words about routing

If you think that what we saw until now is not more than what REAPER's built-in MIDI learn already
offers, I can't blame you. First, don't worry, there's more to come, this was just the beginning.
Second, there _is_ a difference. For some folks, this is an insignificant difference, for others
it's a game changer, it depends on the usage scenario. The key to understand this difference is to
understand the MIDI _routing_: We have just used the so-called _track MIDI path_ to control a
parameter in REAPER. This is different from REAPER's built-in MIDI learn, which uses the _control
MIDI path_.

Using the track MIDI path means it's completely up to you to decide what MIDI messages flow into
ReaLearn. You decide that by using REAPER's powerful routing capabilities. For example, you can
simply "disable" the mapping by disarming your track, a feature that is very desirable if you use
REAPER as live instrument. Or you can preprocess incoming MIDI (although that should rarely be
necessary given ReaLearn's mapping customization possibilities).

Another thing worth to point out which is different from built-in MIDI learn is that we didn't use
the action "Track: Set volume for track 01". Benefit: ReaLearn will let you control the volume of
the track even if you move that track to another position. The track's position is irrelevant!

### Feedback

In ReaLearn, every mapping has 2 directions: _control_ (controller to REAPER) and _feedback_ (REAPER
to controller). So far we have talked about the _control_ direction only: When you move a knob on
your controller, something will happen in REAPER. But if your controller supports it, the other
direction is possible, too!

Imagine you would use a MIDI-controllable motorized fader as control element to change the track
volume. ReaLearn is capable of making that fader move whenever your track volume in REAPER changes -
no matter if that change happens through automation or through dragging the fader with your mouse.
Motorized faders are quite fancy. Another form of feedback visualisation are rotary encoders with
LEDs that indicate the current parameter value.

How to set this up? Often it's just a matter of choosing the correct feedback device:

1. Make sure **Enable output to this device** is checked for your controller MIDI output device in
   the REAPER preferences.
2. In ReaLearn's header panel, select your controller as _MIDI feedback output_.

That should be it!

If it doesn't work and you have ruled out MIDI connection issues, here are some possible causes:

1. **Your controller is not capable of feedback via generic MIDI messages.**
   - Some controllers _do_ support feedback, but not via MIDI. Or via MIDI but using some custom
     sys-ex protocol instead of generic MIDI messages.
   - In this case, ReaLearn can't help you. Reverse engineering custom protocols is out of
     ReaLearn's scope.
   - Recommendation: Maybe you are able to find some bridge driver for your controller that is
     capable of translating generic MIDI messages to the proprietary protocol. Then it could work.
   - Examples: Akai Advance keyboards, Native Instruments Kontrol keyboards, Arturia MiniLab
2. **Your controller has multiple modes and currently is in the wrong one.**
   - Some controllers, especially DAW controllers, are able to work with several protocols.
   - Recommendation: Consult your controller's manual and take the necessary steps to put it into
     something like a "generic MIDI" mode.
   - Example: Presonus Faderport
3. **Your controller expects feedback via messages that are different from the control MIDI messages.**
   - Usually, controllers with feedback support are kind of symmetric. Here's an example what I mean
     by that: Let's assume your motorized fader _emits_ CC 18 MIDI messages when you move it. That
     same motorized fader starts to move when it _receives_ CC 18 MIDI messages (messages of exactly
     the same type). That's what I call symmetric. E.g. it's not symmetric if it emits CC 18 but
     reacts when receiving CC 19.
   - ReaLearn assumes that your controller is symmetric. If it's not, you will observe non-working
     or mixed-up feedback.
   - Recommendation: Consult your controller's manual and try to find out which MIDI messages need
     to be sent to the controller to deliver feedback to the control element in question. Then,
     split your mapping into two, making the first one a control-only and the second one a
     feedback-only mapping. Adjust the source of the feedback-only mapping accordingly. In the next
     section you'll learn how to do that.
   - Example: Presonus Faderport

Personally, I've made good feedback experiences with the following controllers (but I haven't tried
very many, so this is for sure a very incomplete list):

- DJ TechTools Midi Fighter Twister
- Akai APC Key 25
- Presonus Faderport

All hardware examples are provided to the best of my knowledge. If anything is incorrect or has
changed in the meanwhile, please let me know!

### Editing a mapping

When you press the _Edit_ button of a mapping row, a so-called _mapping panel_ appears, which lets
you look at the corresponding mapping in detail and modify it:

![Mapping panel](images/screenshot-mapping-panel.png)

This panel has 4 sections:

- **Mapping:** Allows to change the name and other general settings related to this mapping.
- **Source:** Allows to edit the _source_ of the mapping. In most cases, a source represents a
  particular control element on your controller (e.g. a fader).
- **Target:** Allows to edit the _target_ of the mapping and optionally some target-related
  activation conditions. A target essentially is the parameter in REAPER that should be controlled.
- **Tuning:** Allows to change in detail how your source and target will be glued together. This
  defines _how_ incoming control values from the source should be
  applied to the target (and vice versa, if feedback is used). This is where it gets interesting.
  Whereas REAPER's built-in MIDI learn provides just some basic modes like Absolute or Toggle, ReaLearn
  allows you to customize many more aspects of a mapping.

By design, source, tuning and target are independent concepts in ReaLearn. They can be combined
freely - although there are some combinations that don't make too much sense.

Changes in the mapping panel are applied immediately. Pressing the _OK_ button just closes the
panel.

**Tip:** It is possible to have up to 4 mapping panels open at the same time.

### Controller setup

In order to get the most out of your controller in combination with ReaLearn, you should consider 
the following hints:

- Put your controller's buttons into momentary mode, *not* toggle mode.
- If you are in the lucky situation of owning a controller with endless rotary encoders, by all
  means, configure them to transmit relative values, not absolute ones!
  - Otherwise you can't take advantage of ReaLearn's advanced features for sources emitting 
    relative values, such as the "Step size" or "Speed" setting. Also, preventing parameter jumps
    can never be as effective in absolute mode as in relative mode.

## Reference

So far we've covered the basics. Now let's look into everything in detail.

### Main panel

#### Header panel

The header panel provides the following user interface elements, no matter if the *main mappings* or
*controller mappings* compartment is shown:

- **MIDI control input:** By default, ReaLearn captures MIDI events from _&lt;FX input&gt;_, which
  consists of all MIDI messages that flow into this ReaLearn VSTi FX instance (= track MIDI path).
  Alternatively, ReaLearn can capture events directly from a MIDI hardware input. This dropdown lets
  you choose the corresponding MIDI input device. Be aware that both will only work if _Enable input
  from this device_ is checked for the selected MIDI input device in REAPER's MIDI preferences.
- **MIDI feedback output:** Here you can choose if and where ReaLearn should send MIDI feedback. By
  default it's set to _&lt;None&gt_ for no feedback. If you want to enable feedback, pick a MIDI
  output device here. Keep in mind that _Enable output to this device_ must be checked in REAPER's
  MIDI preferences. As an alternative, you can send feedback to _&lt;FX output&gt;_, which makes
  feedback MIDI events stream down to the next FX in the chain or to the track's hardware MIDI output.
  Tip: Latter option is great for checking which MIDI messages ReaLearn sends to your device. Just add
  a "ReaControlMIDI" FX right below ReaLearn and press "Show Log". Please note that sending MIDI feedback
  to the FX output doesn't work if ReaLearn FX is suspended, e.g. in the following cases:
    - ReaLearn FX is disabled.
    - Project is paused and ReaLearn track is not armed.
    - ReaLearn FX is on input FX chain and track is not armed.
- **Import from clipboard / Export to clipboard:** Pressing the export button copies a _complete_ dump
  of ReaLearn's current settings (including all mappings, even controller mappings) to the clipboard. Pressing the
  import button does the opposite: It restores whatever ReaLearn dump is currently in the clipboard. This
  is a very powerful feature because the dump's data format is
  [JSON](https://www.json.org/json-en.html), a wide-spread data exchange format. It's a text format,
  so if you are familiar with the search&replace feature of your favorite text editor, this is your
  entrance ticket to batch editing. You can also use it for some very basic A/B testing (1. Press
  _Export to clipboard_, 2. change some settings and test them, 3. Restore the old settings by
  pressing _Import from clipboard_). For the programmers and script junkies out there: It's perfectly
  possible to program ReaLearn from outside by passing it a snippet of JSON via [REAPER's named parameter
  mechanism](https://www.reaper.fm/sdk/vst/vst_ext.php) (search for `named_parameter_name`). Parameter name
  is `realearn/set-state"`.
- **Projection:** This is a quite unique feature that allows you to project a schematic representation
  of your currently active controller to a mobile device (e.g. a tablet computer). You can put this device close
  to your controller in order to see immediately which control element is mapped to which parameter.
  This is an attempt to solve an inherent problem with generic controllers: That it's easy to forget which control
  element is mapped to which target parameter. If you want to use this feature, just click this button
  and you will see detailed instructions on how to set this up.
- **Let through:** By default, ReaLearn "eats" MIDI events for which there's at least one enabled mapping source.
  In other words, it doesn't forward MIDI events which are used to control a target parameter. Unmatched MIDI events,
  however, are forwarded to ReaLearn's FX output. This default setting usually makes much sense if you put the
  ReaLearn FX in front of another instrument FX. Use these checkboxes to change that behavior. Please note that this
  refers to MIDI events coming from *FX input* only. MIDI events captured from a MIDI hardware input are never forwarded
  to ReaLearn's FX output.
- **Compartment:** This lets you choose which mapping compartment is displayed. A compartment is a list of mappings
  that can be saved as independent preset. Initially, it shows the list of so-called "Main mappings", which are the
  bread and butter of ReaLearn. However, there's another interesting compartment: "Controller mappings". In a nutshell,
  this compartment lets you define which hardware controllers you have at your disposal and which control elements they
  have. Learn more about that feature in section "Controller mappings".
- **Compartment preset:** This shows the list of available presets for that compartment. By default, this is set to
  "&lt;None&gt;", which means that no particular preset is active. If you select a preset in this list, its
  corresponding mappings will be loaded and immediately get active. In the *controller mappings* compartment, this list
  will essentially represent the list of available hardware controller presets. A few are shipped with ReaLearn itself
  (separately downloadable via ReaPack) but you can also define your own ones and add them to this list!
- **Save:** If you made changes to a preset, you can save them by pressing this button. This works for built-in presets
  as well but I would strongly recommend against changing them directly. Better use *Save as...* and choose a custom
  name.
- **Save as...** This allows you to save all currently visible mappings as a new preset. Please choose a descriptive
  name.
    - Saving your mappings as a preset is optional. All controller mappings are saved together
      with your current ReaLearn instance anyway, no worries. But as soon as you want to reuse these
      mappings in other ReaLearn instances, it makes of course sense to save them as a preset!
    - All of your presets end up in the REAPER resource directory
      (REAPER → Actions → Show action list... → Show REAPER resource path in explorer/finder) at
      `Data/helgoboss/realearn/presets`. They are JSON files and very similar to what you get when you press
      *Export to clipboard*.
    - JSON files that represent controller mappings can also contain custom data sections. For example, the ReaLearn
      Companion app adds a custom data section in order to memorize the positions and shapes of all control elements.
    - When pressing this button, ReaLearn might detect that your current mappings are referring to specific tracks and
      FX instances *within the current project*. This would somehow defeat the purpose of presets because what good
      are presets that are usable only within one project? That's why ReaLearn also offers you to automatically
      convert such mappings to project-independent mappings by applying the following transformations:
        - FX targets are changed to refer to *currently focused FX** instead of a particular one. Their track is set to
          **&lt;This&gt;** because it doesn't matter anyway.
        - Track targets are changed to refer to a track via its position instead of its ID.
    - If this is not what you want, you can choose to say no and make modifications yourself.
- **Delete:** This permanently deletes the currently chosen preset. You can also delete built-in presets.
  However, if you use ReaPack for installation, it should restore them on next sync.
- **Add one:** Adds a new mapping at the end of the current mapping list.
- **Learn many:** Allows you to add and learn many new mappings in a convenient batch mode. Click this button and follow
  the on-screen instructions. Click *Stop* when you are finished with your bulk learning strike.
- **Search:** Enter some text here in order to display just mappings whose name matches the text.
- **Filter source:** If you work with many mappings and you have problems memorizing them, you
  will love this feature. When you press this button, ReaLearn will start listening to incoming MIDI
  events and temporarily disable all target control. You can play around freely on your controller
  without having to worry about messing up target parameters. Whenever ReaLearn detects a valid
  source, it will filter the mapping list by showing only mappings which have that source. This is a
  great way to find out what a specific knob/fader/button etc. is mapped to. Please note that the
  list can end up empty (if no mapping has that source). As soon as you press _Stop_, the current
  filter setting will get locked. This in turn is useful for temporarily focusing on mappings with a
  particular source. When you are done and you want to see all mappings again, press the **X**
  button to the right. _Tip:_ Before you freak out thinking that ReaLearn doesn't work anymore
  because it won't let you control targets, have a quick look at this button. ReaLearn might still
  be in "filter source" mode. Then just calm down and press _Stop_. It's easy to forget.
- **Filter target:** If you want to find out what mappings exist for a particular target,
  press this button and touch something in REAPER. As soon as you have touched a valid target, the
  list will show all mappings with that target. Unlike _Filter source_, ReaLearn will
  automatically stop learning as soon as a target was touched. Press the **X** button to remove the
  filter and show all mappings again.

Additionally, the header panel provides a context menu with the following entries:

- **Options**
    - **Auto-correct settings:** By default, whenever you change something in ReaLearn, it tries to
      figure out if your combination of settings makes sense. If not, it makes an adjustment.
      This auto-correction is usually helpful. If for some reason you want to disable auto-correction, this
      is your checkbox.
    - **Send feedback only if track armed:** If MIDI control input is set to _&lt;FX input&gt;_,
      ReaLearn by default only sends feedback if the track is armed (unarming will naturally disable
      control, so disabling feedback is just consequent). However, if MIDI control input is set to a
      hardware device, *auto-correct settings* will take care of unchecking this option in order to allow feedback
      even when unarmed (same reasoning). You can override this behavior with this checkbox. At the moment,
      it can only be unchecked if ReaLearn is on the normal FX chain. If it's on the input FX chain, unarming
      naturally disables feedback because REAPER generally excludes input FX from audio/MIDI processing while a
      track is unarmed (*this is subject to change in future!*).
- **Server**
    - **Enabled:** This enables/disables the built-in server for allowing the ReaLearn companion app to
      connect to ReaLearn.
    - **Add firewall rule:** Attempts to add a firewall rule for making the server accessible from other devices or
      displays instructions how to do it.
    - **Change session ID...:** This lets you customize the ID used to address this particular ReaLearn
      instance when using the projection feature.
        - By default, the session ID is a random cryptic string
          which ensures that every instance is uniquely addressable. The result is that scanning the QR code
          of this ReaLearn instance will let your mobile device connect for sure with this unique 
          instance, not with another one - remember, you can use many instances of ReaLearn in parallel. This
          is usually what you want.
        - But a side effect is that with every new ReaLearn instance that you create,
          you first have to point your mobile device to it in order to see its
          projection (by scanning the QR code). Let's assume you have in many of your projects exactly one ReaLearn instance
          that lets your favorite MIDI controller control track volumes. By customizing the session ID, you basically can tell
          your mobile device that it should always show the projection of this very ReaLearn instance -
          no matter in which REAPER project you are and even if they control the volumes of totally
          different tracks.
        - You can achieve this by setting the session ID of each volume-controlling ReaLearn instance
          to exactly the same value, in each project. Ideally it's a descriptive name without spaces, such as "track-volumes".
          You have to do the pairing only once et voilà, you have a dedicated device for monitoring your volume control
          ReaLearn instances in each project.
        - **Make sure to not have more than one ReaLearn instance with the same session 
          ID active at the same time because then it's not clear to which your mobile device will connect!**
        - **At the moment, the session ID is part of the ReaLearn preset!** That means, opening a preset, copying/cutting
          a ReaLearn FX, importing from clipboard - all of that will overwrite the session ID. This might change in
          future in favor of a more nuanced approach!
- **Help:** As the name says.
- **Log debug info:** Logs some information about ReaLearn's internal state. Can be interesting for
  investigating bugs or understanding how this plug-in works.
- **Send feedback now:** Usually ReaLearn sends feedback whenever something changed to keep the LEDs
  or motorized faders of your controller in sync with REAPER at all times. There might be situations
  where it doesn't work though. In this case you can send feedback manually using this button. 

#### More about "Controller mappings"

By default, ReaLearn shows the list of main mappings. If you select *Controller mappings* in the *Compartment*
dropdown, you will see the list of controller mappings instead. Each controller mapping represents a control
element on your hardware controller, e.g. a button or fader. This view lets you describe your controller by - well -
by adding mappings. Almost everything in ReaLearn is a mapping :)

Defining your own controllers can have a bunch of benefits:

- You can use the awesome [controller projection feature](https://www.youtube.com/watch?v=omuYBznEShk&feature=youtu.be)
  to project your controller mapping to your smartphone or tablet.
- You can use controller presets made by other users and thereby save precious setup time. Or you can contribute them
  yourself!
- You can make your main mappings independent of the actual controller that you use. This is done using so-called
  *virtual* sources and targets.
- It allows you to give your knobs, buttons etc. descriptive and friendly names instead of just e.g. "CC 15".
- You don't need to learn your control elements again and again. Although the process of learning an element is easy
  in ReaLearn, it can take some time in case the source character is not guessed correctly
  (e.g. absolute range element or relative encoder). Just do it once and be done with it!

If you want to make ReaLearn "learn" about your nice controller device, all you need to do is to create a suitable
controller mapping for each of its control elements.

Let's first look at the "slow" way to do this - adding and editing each controller mapping one by one:

1. Press the "Add one" button.
1. Learn the source by pressing the "Learn source" button and touching the control element.
1. Press the "Edit" button.
1. Enter a descriptive name for the control element.
    - *Hint:* This name will appear in many places so you want it to be short, clear and unique!
1. Assign a unique virtual target.
    - At this point we don't want to assign a *concrete* target yet. The point of controller presets is
      to make them as reusable as possible, that's why we choose a so-called *virtual* target.
    - In the *Category* dropdown, choose *Virtual*.
    - As *Type*, choose *Button* if your control element is a sort of button (something which you can press)
      and *Multi* in all other cases.
    - Use for each control element a unique combination of *Type* and *Number*, starting with number *1* and counting.
        - Example: It's okay and desired to have one control element mapped to "Multi 1" and one to "Button 1".
    - Just imagine the "8 generic knobs + 8 generic buttons" layout which is typical for lots of popular controllers.
      You can easily model that by assigning 8 multis and 8 buttons.
    - Maybe you have realized that the *Tuning* section is available for controller mappings as well! That opens up all
      kinds of possibilities. You could for example restrict the target range for a certain control element. Or make
      an encoder generally slower or faster. Or you could simulate a rotary encoder by making two buttons on your
      controller act as -/+ buttons emitting relative values. This is possible by mapping them to the same "Multi" in
      "Incremental buttons" mode.
      
Before you go ahead and do that for each control element, you might want to check out what this is good for: Navigate
back to the "main mappings" compartment, learn the source of some main mapping and touch the control element that you
have just mapped: Take note how ReaLearn will assign a *virtual* source this time, not a MIDI source! It will also
display the name of the control element as source label. Now, let's say at some point you swap your controller device
with another one that has a similar layout, all you need to do is switch the controller preset and you are golden! You
have decoupled your main mappings from the actual controller. Plus, as soon as you have saved your controller mappings
as a preset, you can take full advantage of the *Projection* feature.

All of this might be a bit of an effort but it's well worth it! Plus, there's a way to do this *a lot* faster by
using *batch learning*:

1. Press the "Learn many" button.
2. Choose whether you want to learn all the "Multis" on your controller or all the "Buttons".
3. Simply touch all control elements in the desired order.
    - ReaLearn will take care of automatically incrementing the virtual control element numbers.
4. Press "Stop".
5. Done!
    - At this point it's recommended to recheck the learned mappings. 
    - ReaLearn's source character detection for MIDI CCs is naturally just a guess, so it can be wrong. If so,
      just adjust the character in the corresponding mapping panel.

You can share your preset with other users by sending them to info@helgoboss.org. I will add it to [this
list](https://github.com/helgoboss/realearn/tree/master/resources/controllers).

#### More about "Main mappings"

The header panel for main mappings consists of a few more user interface elements that you might find immensely
helpful:

- **Mapping group:** Mapping groups allow you to divide your list of main mappings into multiple groups.
    - Groups can be useful ... 
        - To apply an activation condition to multiple mappings at once. 
        - To enable/disable control/feedback for multiple mappings at once.
        - To keep track of mappings if there are many of them.
    - This dropdown contains the following options:
        - **&lt;All&gt;:** Displays all mappings in the compartment, no matter to which group they belong. 
        - **&lt;Default&gt;:** Displays mappings that belong to the *default* group. This is where mappings
          end up if you don't care about grouping. This is a special group that can't be removed.
        - ***Custom group*:** Displays all mappings in your custom group.
    - You can move existing mappings between groups by opening the context menu of the corresponding mapping row.
    - Groups are saved as part of the project, VST plug-in preset and compartment preset.
- **Add:** Allows you to add a group and give it a specific name.
- **Remove:** Removes the currently displayed group. It will ask you if you want to remove all the mappings in that
  group as well. Alternatively they will automatically be moved to the default group.
- **Edit:** Opens the group panel. This allows you to change the group name, enable/disable control and/or
  feedback and set an activation condition for all mappings in this group. The activation condition that you provide
  here is combined with the one that you provide in the mapping. Only if both, the group activation conditions and
  the mapping activation condition are satisfied, the corresponding mapping will be active. Read more about
  *conditional activation* below in the section about the *mapping panel*.

![Group panel](images/screenshot-group-panel.png)

- **Auto-load preset:** If you switch this to *Depending on focused FX*, ReaLearn will start to observe which
  FX window is currently focused. Whenever the focus changes, it will check if you have linked a compartment preset
  to it and will automatically load it. Whenever a non-linked FX gets focus, the mapping list is cleared so that
  no mapping is active anymore. Of course this makes sense only if you actually have linked some presets. Read on!

The header context menu for the main mapping compartment contains the missing piece of the puzzle:  
- **Link current preset to FX / Unlink current preset from FX:** This lets you link the currently active compartment
  preset with whatever FX window was focused before focusing ReaLearn. This only works if a preset is active and an
  FX has been focused before. If the active preset is already linked to an FX, you can unlink it. 

#### Mapping row

The source and target label of a mapping row is greyed out whenever the mapping is *off*. A mapping is considered as 
*on* only if the following is true:

1. The mapping is complete, that is, both source and target are completely specified.
2. The mapping has control and/or feedback enabled.
3. The mapping is active (see *conditional activation*).

If a mapping is *off*, it doesn't have any effect.

- **Up / Down:** Use these buttons to move this mapping up or down the list.
- **→ / ←:** Use these checkboxes to enable/disable control and/or feedback for this mapping.
- **Edit:** Opens the mapping panel for this mapping.
- **Duplicate:** Creates a new mapping just like this one right below.
- **Remove:** Removes this mapping from the list.
- **Learn source:** Starts or stops learning the source of this mapping.
- **Learn target:** Starts or stops learning the target of this mapping.

Each mapping row provides a context menu, which lets you move this mapping to another mapping group.

### Mapping panel

At this point it's important to understand some basics about how ReaLearn processes incoming control
events. When there's an incoming control event that matches a particular source, one of the first
things ReaLearn does is to normalize it to a so-called _control value_.

A control value can be either absolute or relative, depending on the source character:

- **Source emits absolute values (e.g. faders)**: The control value will be absolute, which means
  it's a 64-bit decimal number between 0.0 and 1.0. You can also think of it in terms of
  percentages: Something between 0% and 100%. 0% means the minimum possible value of the source has
  been emitted whereas 100% means the maximum.
- **Source emits relative values (e.g. rotary encoders)**: The control value will be relative, which
  means it's a positive or negative integer that reflects the amount of the increment or decrement.
  E.g. -2 means a decrement of 2.

After having translated the incoming event to a control value, ReaLearn feeds it to the mapping's
tuning section. The tuning section is responsible for transforming control values before they reach the _target_.
This transformation can change the type of the control value, e.g. from relative to absolute - it depends
on the mapping's target character. The tuning section can even "eat" control values so that they don't arrive
at the target at all.

Finally, ReaLearn converts the transformed control value into some target instruction (e.g. "set
volume to -6.0 dB") and executes it.

Feedback (from REAPER to controller) works in a similar fashion but is restricted to absolute
control values. Even if the source is relative (e.g. an encoder), ReaLearn will always emit absolute
feedback, because relative feedback doesn't make sense.

#### Mapping

This section provides the following mapping-related settings and functions:

- **Name:** Here you can enter a descriptive name for the mapping. This is especially useful in
  combination with the search function if there are many mappings to keep track of.
- **Control enabled / Feedback enabled:** Use these checkboxes to enable/disable control and/or
  feedback for this mapping.
- **Active:** This dropdown controls so-called conditional activation of mappings. See section below.
- **Prevent echo feedback:** This checkbox mainly exists for motorized faders that don't like
  getting feedback while being moved. If checked, ReaLearn won't send feedback if the target value
  change was caused by incoming source events of this mapping. However, it will still send feedback
  if the target value change was caused by something else, e.g. a mouse action within REAPER itself.
- **Send feedback after control:** This checkbox mainly exists for "fixing" controllers which allow
  their LEDs to be controlled via incoming MIDI *but at the same time* insist on controlling these 
  LEDs themselves. According to users, some Behringer X-Touch Compact buttons exhibit this behavior,
  for example. This can lead to wrong LED states which don't reflect the actual state in REAPER.
  If this checkbox is not checked (the normal case and recommended for most controllers), ReaLearn 
  will send feedback to the controller *only* if the target value has changed. For example, if you
  use a button to toggle a target value on and off, the target value will change only when pressing
  the button, not when releasing it. As a consequence, feedback will be sent only when pressing the
  button, not when releasing it. However, if this checkbox is checked, ReaLearn will send feedback
  even after releasing the button - although the target value has not been changed by it. Another
  case where this option comes in handy is if you use a target which doesn't support proper feedback
  because REAPER doesn't notify ReaLearn about value changes (e.g. "Track FX all enable"). By
  checking this checkbox, ReaLearn will send feedback whenever the target value change was caused
  by ReaLearn itself, which improves the situation at least a bit.
- **Find in mapping list:** Scrolls the mapping rows panel so that the corresponding mapping row for
  this mapping gets visible.
  
#### Conditional activation

Conditional activation allows you to dynamically enable or disable this mapping based on the state of
ReaLearn's own plug-in parameters. This is a powerful feature. It is especially practical if your
controller has a limited amount of control elements and you want to give control elements several
responsibilities. It let's you easily implement use cases such as:

- "This knob should control the track pan, but only when my sustain pedal is pressed, otherwise it 
  should control track volume!"
- "I want to have two buttons for switching between different programs where each program represents
  a group of mappings."

There are 4 different activation modes:

- **Always:** Mapping is always active (the default)
- **When modifiers on/off:** Mapping becomes active only if something is pressed / not pressed
- **When program selected:** Allows you to step through different groups of mappings
- **When EEL result > 0:** Let a formula decide (total freedom)

For details, see below.

At this occasion some words about ReaLearn's plug-in parameters. ReaLearn itself isn't just able to
control parameters of other FX, it also offers FX parameters itself. At the moment these are
"Parameter 1" to "Parameter 100". You can control them just like parameters in other FX: Via automation
envelopes, via track controls, via REAPER's own MIDI learn ... and of course via ReaLearn itself.
Initially, they don't do anything at all. First, you need to give meaning to them by referring to them
in conditional activation. In future, ReaLearn will provide additional ways to make use of parameters.

##### When modifiers on/off

This mode is comparable to modifier keys on a computer keyboard. For example, when you press `Ctrl+V`
for pasting text, `Ctrl` is a modifier because it modifies the meaning of the `V` key. When this
modifier is "on" (= pressed), it activates the "paste text" and deactivates the "write the letter V"
functionality of the `V` key.

In ReaLearn, the modifier is one of the FX parameters. It's considered to be "on" if the parameter
has a value greater than 0 and "off" if the value is 0.

You can choose up to 2 modifier parameters, "Modifier A" and "Modifier B". If you select "&lt;None&gt;",
the modifier gets disabled (it won't have any effect on activation). The checkbox to the right of
the dropdown lets you decide if the modifier must be "on" for the mapping to become active or "off".

Example: The following setting means that this mapping becomes active *only* if both "Parameter 1"
and "Parameter 2" are "on".

- **Modifier A:** "Parameter 1"
- **Checkbox A:** Checked
- **Modifier B:** "Parameter 2"   
- **Checkbox B:** Checked

Now you just have to map 2 controller buttons to "Parameter 1" and "Parameter 2" via ReaLearn (by
creating 2 additional mappings - in the same ReaLearn instance or another one, up to you) et voilà,
it works. The beauty of this solution lies in how you can compose different ReaLearn features to
obtain exactly the result you want. For example, the *absolute mode* of the mapping that controls the modifier
parameter decides if the modifier button is momentary (has to be pressed all the time)
or toggled (switches between on and off everytime you press it). You can also be more adventurous
and let the modifier on/off state change over time, using REAPER's automation envelopes.

##### When program selected

You can tell ReaLearn to only activate your mapping if a certain parameter has a particular value.
The certain parameter is called "Bank" and the particular value is called "Program". Why? Let's
assume you mapped 2 buttons "Previous" and "Next" to increase/decrease the value of the "Bank" parameter 
(by using "Incremental buttons" mode, you will learn how to do that further below). And you have multiple
mappings where each one uses "When program selected" with the same "Bank" parameter but a different "Program".
Then the result is that you can press "Previous" and "Next" and it will switch between different 
mappings (programs) within that bank. If you assign the same "Program" to multiple mappings, it's like putting
those mapping into one group which can be activated/deactivated as a whole.

Switching between different programs via "Previous" and "Next" buttons is just one possibility.
Here are some other ones:

- **Navigate between programs using a rotary encoder:** Just map the rotary encoder
  to the "Bank" parameter and restrict the target range as desired.
- **Activate each program with a separate button:** Map each button to the "Bank"
  parameter (with absolute mode "Normal") and set "Target Min/Max" to a distinct value. E.g. set button
  1 min/max both to 0% and button 2 min/max both to 1%. Then pressing button 1
  will activate program 0 and pressing button 2 will activate program 1.

In previous versions of ReaLearn you could use other methods to achieve a similar behavior, but it always
involved using multiple ReaLearn instances:

- **By enabling/disabling other ReaLearn instances:** You can use one main ReaLearn instance containing
  a bunch of mappings with "Track FX enable" target in order to enable/disable other ReaLearn FX
  instances. Then each of the other ReaLearn instances acts as one mapping bank/group.
- **By switching between presets of another ReaLearn instance:** You can use one main ReaLearn instance
  containing a mapping with "Track FX preset" target in order to navigate between presets of another
  ReaLearn FX instance. Then each preset in the other ReaLearn instance acts as one mapping bank/group.
  However, that method is pretty limited and hard to maintain because presets are something global
  (not saved together with your REAPER project).

With *Conditional activation* you can do the same (and more) within just one ReaLearn instance. A fixed
assumption here is that each bank (parameter) consists of 100 programs. If this is too limiting for you,
please use the EEL activation mode instead.    

##### When EEL result > 0

This is for experts. It allows you to write a formula in [EEL2](https://www.cockos.com/EEL2/) language
that determines if the mapping becomes active or not, based on potentially all parameter values.
This is the most flexible of all activation modes. The other modes can be easily simulated. The example
modifier condition scenario mentioned above written as formula would be:

```
y = p1 > 0 && p2 > 0
```

`y` represents the result. If `y` is greater than zero, the mapping will become active, otherwise
it will become inactive. `p1` to `p100` contain the current parameter values. Each of them has a
value between 0.0 (= 0%) and 1.0 (= 100%).

This activation mode accounts for ReaLearn's philosophy to allow for great flexibility instead of just implementing
one particular use case. If you feel limited by the other activation modes, just use EEL.  

##### Custom parameter names

There's a somewhat hidden possibility to give ReaLearn parameters more descriptive names (yes, not
very convenient, hopefully future versions will improve on that):

1. Press *Export to clipboard* in the main panel.
2. Paste the result into a text editor of your choice.
3. You will see a property "parameters", e.g.
   ```json
   "parameters": {
     "0": {
       "value": 0.084
     }
   }
   ```
4. Adjust it as you like, e.g.
   ```json
   "parameters": {
     "0": {
       "value": 0.084,
       "name": "Pedal"
     },
     "1": {
       "name": "Shift"
     }
   }
   ```
5. Copy the complete text to the clipboard.
6. Press *Import from clipboard*  in the main panel.
   
Parameter names are not global, they are always saved together with the REAPER project / FX preset /
track template etc.

##### Use case: Control A when a button is not pressed, control B when it is

Here's how you would implement a typical use case. You want your rotary encoder to control target A when the button is
not pressed and control target B when it's pressed.

1. Create a mapping for the button
    - As "Target", you need to choose ReaLearn itself (Type: "Track FX parameter", Track: `<This>`, FX: "... VSTi: ReaLearn (Helgoboss)"). As "Parameter", choose an arbitrary ReaLearn parameter, e.g. "Parameter 1". 
    - As "Mode", choose either "Absolute" (if you want to switch the encoder function just momentarily) or "Toggle" (if you want the button to toggle between the two encoder functions).
1. Create a mapping with target A
    - Set "Active" to "When modifiers on/off", "Modifier A" to "Parameter 1" and disable the checkbox beside it. Set "Modifier B" to `<None>`.
    - This basically means "Hey, ReaLearn! Please activate this mapping only if ReaLearn Parameter 1 is **off**!" (remember, we control ReaLearn Parameter 1 using the button).
    - At this point, turning your encoder should control target A, but only if you don't press the button!
1. Create a mapping with target B
    - Just as in step 2, set "Active" to "When modifiers on/off" and "Modifier A" to "Parameter 1". **But**: Now **enable** the checkbox beside it. Set "Modifier B" to `<None>`.
    - This basically means "Hey, ReaLearn! Please activate this mapping only if ReaLearn Parameter 1 is **on**!"
    - At this point, turning your encoder should control target A if you don't press the button and control target B if you press the button.

#### Source

As mentioned before, a source usually represents a single control element on your controller.
Sources share the following common settings and functions:

- **Learn:** Starts or stops learning the source of this mapping.
- **Category:** Lets you choose the source category.
    - **MIDI:** Incoming MIDI events.
    - **Virtual:** Invocations of virtual control elements (coming from controller mappings). This source
      category is available for main mappings only. 
- **Type:** Let's you choose the source type. Available types depend on the selected category.
  
All other UI elements in this section depend on the chosen category. 

##### Category "MIDI"

All types in the MIDI category have the following UI elements in common:

- **Channel:** Optionally restricts this source to messages from a certain MIDI channel. Only
  available for sources that emit MIDI channel messages.

The remaining UI elements in this section depend on the chosen source type.

###### CC value source

This source reacts to incoming MIDI control-change messages.

- **CC:** Optionally restricts this source to messages with a certain MIDI control-change controller
  number.
- **Character:** MIDI control-change messages (7-bit ones) serve a very wide spectrum of MIDI
  control use cases. Even though some control-change controller numbers have a special purpose
  according to the MIDI specification (e.g. CC 7 = channel volume), nothing prevents one from using
  them for totally different purposes. In practice that happens quite often, especially when using
  general-purpose controllers. Also, there's no strict standard whatsoever that specifies how
  relative values (increments/decrements) shall be emitted and which controller numbers emit them.
  Therefore you explicitly need to tell ReaLearn about it by setting the _source character_. The
  good news is: If you use "Learn source", ReaLearn will try to guess the source character for you
  by looking at the emitted values. Naturally, the result is not always correct. The best guessing
  result can be achieved by turning the knob or encoder quickly and "passionately" into clockwise
  direction. Please note that guessing doesn't support encoder type 3. The possible values are:
  - **Range element (knob, fader, etc.):** A control element that emits continuous absolute values. Examples: Faders,
    knobs, modulation wheel, pitch bend, ribbon controller.
  - **Button (momentary):** A control element that can be pressed and emits absolute values. It emits a > 0%
    value when pressing it and optionally a 0% value when releasing it. Examples: Damper pedal.
    - Hint: There's no option "Button (toggle)" because ReaLearn can only take full control with momentary
      buttons. So make sure your controller buttons are in momentary mode! ReaLearn itself provides
      a toggle mode that is naturally more capable than your controller's built-in toggle mode.
  - **Encoder (type _x_):** A control element that emits relative values, usually an endless rotary
    encoder. The _x_ specifies _how_ the relative values are sent. This 1:1 corresponds to the
    relative modes in REAPER's built-in MIDI learn:
    - **Type 1**:
      - 127 = decrement; 0 = none; 1 = increment
      - 127 > value > 63 results in higher decrements (64 possible decrement amounts)
      - 1 < value <= 63 results in higher increments (63 possible increment amounts)
    - **Type 2**:
      - 63 = decrement; 64 = none; 65 = increment
      - 63 > value >= 0 results in higher decrements (64 possible decrement amounts)
      - 65 < value <= 127 results in higher increments (63 possible increment amounts)
    - **Type 3**:
      - 65 = decrement; 0 = none; 1 = increment
      - 65 < value <= 127 results in higher decrements (63 possible decrement amounts)
      - 1 < value <= 64 results in higher increments (64 possible increment amounts)
- **14-bit values:** If unchecked, this source reacts to MIDI control-change messages with 7-bit
  resolution (usually the case). If checked, it reacts to MIDI control-change messages with 14-bit
  resolution. This is not so common but sometimes used by controllers with high-precision faders.

###### Note velocity source

This source reacts to incoming MIDI note-on and note-off messages. The higher the velocity of the
incoming note-on message, the higher the absolute control value. Note-off messages are always
translated to 0%, even if there's a note-off velocity.

- **Note:** Optionally restricts this source to messages with a certain note number (note numbers
  represent keys on the MIDI keyboard, e.g. 60 corresponds to C4).

###### Note number source

This source reacts to incoming MIDI note-on messages. The higher the note number (= key on a MIDI
keyboard), the higher the absolute control value.

This essentially turns your MIDI keyboard into a "huge fader" with the advantage that you can jump
to any value at any time.

###### Pitch wheel source

This source reacts to incoming MIDI pitch-bend change messages. The higher the pitch-wheel position,
the higher the absolute control value. The center position corresponds to an absolute control value
of 50%.

###### Channel after touch source

This source reacts to incoming MIDI channel-pressure messages. The higher the pressure, the higher
the absolute control value.

###### Program change source

This source reacts to incoming MIDI program-change messages. The higher the program number, the
higher the absolute control value.

###### (N)RPN value source

This source reacts to incoming non-registered (NRPN) or registered (RPN) MIDI parameter-number
messages. The higher the emitted value, the higher the absolute control value.

(N)RPN messages are not widely used. If they are, then mostly to take advantage of their ability to
transmit 14-bit values (up to 16384 different values instead of only 128), resulting in a higher
resolution.

- **Number:** The number of the registered or unregistered parameter-number message. This is a value
  between 0 and 16383.
- **RPN:** If unchecked, this source reacts to unregistered parameter-number messages (NRPN). If
  checked, it reacts to registered ones (RPN).
- **14-bit values:** If unchecked, this source reacts to (N)RPN messages with 7-bit resolution. If
  checked, it reacts to those with 14-bit resolution. In practice, this if often checked.

###### Polyphonic after touch source

This source reacts to incoming MIDI polyphonic-key-pressure messages. The higher the pressure, the
higher the absolute control value.

- **Note:** Optionally restricts this source to messages with a certain note number.

###### MIDI clock tempo source

This source reacts to incoming MIDI clock (MTC) tempo messages. These are metronome-beat-like
messages which can be regularly transmitted by some DAWs and MIDI devices. The frequency with which
this message is sent dictates the tempo.

The higher the calculated tempo, the higher the absolute control value. A tempo of 1 bpm will be
translated to a control value of 0%, a tempo of 960 bpm to 100% (this corresponds to REAPER's
supported tempo range).

This source can be used in combination with the _Master tempo_ target to obtain a "poor man's" tempo
synchronization. Be aware: MIDI clock naturally suffers from certain inaccuracies and latencies -
that's an issue inherent to the nature of the MIDI clock protocol itself. E.g. it's not really
suitable if you need super accurate and instant tempo synchronization. Additionally, ReaLearn's
algorithm for calculating the tempo could probably be improved (that's why this source is marked as
experimental).

###### MIDI clock transport source

This source reacts to incoming MIDI clock (MTC) transport messages. These are simple start, continue
and stop messages which can be sent by some DAWs and MIDI devices.

- **Message:** The specific transport message to which this source should react.

##### Category "Virtual"

As pointed out before, *virtual* sources exist in order to decouple your mappings from the actual
MIDI source.


The following virtual source types are kind of the lowest common denominator of possible controls. They
are inspired by the popular 8-knobs/8-buttons layout. Sometimes the knobs are just knobs, sometimes they are rotary
encoders - it doesn't really matter in most cases. The most important distinction is "something which you can move"
or "something which you can press".

Both types have:

- **Number:** The logical number of the control element. In a row of 8 knobs one would typically assign number 1 to the
  leftmost one and number 8 to the rightmost one. It's your choice.

###### Multi

Represents a control element that you can "move", that is, something that allows you to choose between more than 2
values. Usually everything which is *not* a button :) Here's a list of typical *multis*:  

- Fader
- Knob
- Pitch wheel
- Mod wheel
- Endless encoder
- XY pad (1 axis)
- Touch strip
- (Endless) rotary encoder

###### Button

Represents a control element that you can "press", that is, something which is just a trigger or has only 2 states
(on/off). Usually everything which is a button. Here's a list of typical *multis*:

- Play button
- Switch
- Sustain pedal

It's not 100% clear sometimes, e.g. velocity-sensitive keys could be either a multi or a button. Choose what is suited
best for your musical use case. 

#### Target

A target is a thing that is supposed to be controlled. The following settings and functions are
shared among all targets:

- **Category:** Lets you choose the target category.
    - **REAPER:** Targets that are about changing something in REAPER.
    - **Virtual:** Targets that invoke virtual control elements. This source
      category is available for controller mappings only. 
- **Learn:** Starts or stops learning the target of this mapping. 
- **Go there:** If applicable, pressing this button makes the target of this mapping visible in
  REAPER. E.g. if the target is a track FX parameter, the corresponding track FX window will be
  displayed.
- **Type:** Let's you choose the target type.

##### Category "REAPER"

REAPER targets additionally have this:
- **Value:** Reflects the current value of this mapping target and lets you change it.

Only available for targets that are associated with a particular REAPER track:

- **Track:** The track associated with this target. In addition to concrete tracks, the following
  options are possible:
  - **&lt;This&gt;**: Track which hosts this ReaLearn instance. If ReaLearn is on the monitoring FX
    chain, this resolves to the master track of the current project.
  - **&lt;Selected&gt;**: Currently selected track.
  - **&lt;Master track&gt;**: Master track of the project which hosts this ReaLearn instance. If
    ReaLearn is on the monitoring FX chain, this resolves to the master track of the current
    project.
- **Track anchor:** If you select a concrete track, another dropdown will appear to the right of the
  track dropdown. It lets you choose how ReaLearn will identify your track.
  - **By ID:** Refers to the track by its unique ID (the default). Choose this if you want ReaLearn to always control this
    very particular track even in case you move it somewhere else or rename it. Please note that it's *not possible*
    with this setting to create a ReaLearn preset that is reusable among different projects. Because a track ID
    is globally unique, even across projects. That also means it doesn't make sense to use this setting in a
    ReaLearn monitoring FX instance.
  - **Ny name:** Refers to the track by its name. In case there are multiple tracks with the same name, it will
    always prefer the first one. This will allow you to use one ReaLearn preset across multiple projects that
    have similar naming schemes, e.g. as monitoring FX.
  - **By position:** Refers to the track by its position in the track list. This will allow preset reuse as well.
  - **By ID or name:** This refers to the track by its unique ID with its name as fallback. This was the default
    behavior for ReaLearn versions up to 1.11.0 and is just kept for compatibility reasons.
- **Track must be selected:** If checked, this mapping will be active only if the track set in
  _Track_ is currently selected. Of course, this doesn't have any effect if latter is
  _&lt;Selected&gt;_.

Only available for targets associated with a particular track send:

- **Send:** The (outgoing) send (to another track) associated with this target.

Only available for targets associated with a particular FX instance:

- **FX:** The FX instance associated with this target. In addition to concrete FX instances, the following options are
  possible:
    - **&lt;Focused&gt;**: Currently or last focused FX. *Track* and *Input FX* settings are ignored.
- **FX anchor:** If you select a concrete FX, another dropdown will appear to the right of the
  FX dropdown. It lets you choose how ReaLearn will identify your FX instance.
  - **By ID:** Refers to the FX instance by its unique ID (the default). Choose this if you want ReaLearn to always control
    this very particular FX instance even in case you move it somewhere else within the FX chain or rename it.
  - **By name:** Refers to the FX instance by its name. In case there are multiple instances with the same name, it will
    always prefer the first one.
  - **By position:** Refers to the FX instance by its position within the FX chain.
  - **By ID or position:** This refers to the FX by its unique ID with its position as fallback. This was the default
    behavior for ReaLearn versions up to 1.11.0 and is just kept for compatibility reasons.
- **Input FX:** If unchecked, the _FX_ dropdown will show FX instances in the track's normal FX
  chain. If checked, it will show FX instances in the track's input FX chain.
- **FX must have focus:** If checked, this mapping will be active only if the FX instance set in
  _FX_ is currently focused. If the FX instance is displayed in a floating window, _focused_ means
  that the floating window is active. If it's displayed within the FX chain window, _focused_ means
  that the FX chain window is currently open and the FX instance is the currently selected FX in
  that FX chain. Of course, this flag doesn't have any effect if you chose _&lt;Focused&gt;_ FX.

All other UI elements in this section depend on the chosen target type.

###### Action target

Triggers or sets the value of a particular REAPER action in the main section.

- **Pick:** Opens REAPER's action dialog so you can select the desired action.
- **Invocation type:** Specifies _how_ the picked action is going to be controlled.
  - **Trigger:** Invokes the action with the incoming absolute control value, but only if it's
    greater than 0%. Most suitable for simple trigger-like actions that neither have an on/off state
    nor are annotated with "(MIDI CC/OSC only)" or similar.
  - **Absolute:** Invokes the action with the incoming absolute control value, even if it's 0%. Most
    suitable for actions which either have an on/off state or are annotated with "(MIDI CC/OSC
    only)" or similar.
  - **Relative:** Invokes the action with the incoming relative control value (absolute ones are
    ignored). Only works for actions that are annotated with ("MIDI CC relative only") or similar.

###### Track FX parameter target

Sets the value of a particular track FX parameter.

- **Parameter:** The parameter to be controlled.

###### Track volume target

Sets the track's volume.

###### Track send volume target

Sets the track send's volume.

###### Track pan target

Sets the track's pan value.

###### Track arm target

Arms the track for recording if the incoming absolute control value is greater than 0%, otherwise
disarms the track. This disables "Automatic record-arm when track selected". If you don't want that,
use the _Track selection_ target instead.

###### Track selection target

Selects the track if the incoming absolute control value is greater than 0%, otherwise unselects the
track.

- **Select exclusively:** If unchecked, this leaves all other tracks' selection state untouched. If
  checked, this unselects all other tracks when selecting this track.

###### Track mute target

Mutes the track if the incoming absolute control value is greater than 0%, otherwise unmutes the
track.

###### Track solo target

Soloes the track if the incoming absolute control value is greater than 0%, otherwise unsoloes the
track.

###### Track send pan target

Sets the track send's pan value.

###### Master tempo target

Sets REAPER's master tempo.

###### Master playrate target

Sets REAPER's master playrate.

###### Track FX enable target

Enables the FX instance if the incoming absolute control value is greater than 0%, otherwise
disables it.

###### Track FX preset target

Steps through FX presets.

###### Selected track target

Steps through tracks.

###### Track FX all enable

Enables all the track's FX instances if the incoming absolute control value is greater than
0%, otherwise disables them.

###### Transport

Invokes a transport-related action.

- **Action:** Specifies which transport action should be invoked.
  - **Play/stop:** Starts playing the containing project if the incoming absolute control value is greater than 0%, 
    otherwise invokes stop.
  - **Play/pause:** Starts playing the containing project if the incoming absolute control value is greater than 0%, 
    otherwise invokes pause.
  - **Record:** Starts/enables recording for the current project if the incoming absolute control value is greater than 
    0%, otherwise disables recording.
  - **Repeat:** Enables repeat for the containing project if the incoming absolute control value is greater than 0%, 
    otherwise disables it.

##### Category "Virtual"

This is exactly the counterpart of the possible virtual sources. Choosing a virtual target here is like
placing cables between a control element and all corresponding main mappings that use this
virtual control element as source.      

#### Tuning

As mentioned before, the tuning section defines the glue between a source and a target. It's divided into
several sub sections some of which make sense for all kinds of sources and others only for some.

**At first something important to understand:** Since ReaLearn 1.12.0, a mapping can deal with both *absolute*
and *relative* values, no matter what's set as *Mode*! ReaLearn checks the type of each emitted source value
and interprets it correctly. The *Mode* dropdown has been sort of "degraded" because now it only applies to
incoming *absolute* values and determines how to handle them (see further below). This change has been made 
to support virtual sources - because virtual sources can be either absolute or relative depending on the current 
controller mappings. ReaLearn allows you to prepare your mapping for both cases by showing all possible settings.

*Relative* means that the current target value is relevant and the change of the target value is calculated in
terms of increments or decrements. Control elements that can emit relative values are rotary encoders and
virtual multis.

No matter which kind of source, the following UI elements are always relevant:

- **Reset to defaults:** Resets the settings to some sensible defaults. 

##### For all source characters (control and feedback)

The following elements are relevant for all kinds of sources, both in *control* and *feedback* direction.

- **Target Min/Max:** The controlled range of absolute target values. This enables you to "squeeze"
  target values into a specific value range. E.g. if you set this to "-6 dB to 0 dB" for a _Track
  volume_ target, the volume will always stay within that dB range if controlled via this mapping.
  This wouldn't prevent the volume from exceeding that range if changed e.g. in REAPER itself. This
  setting applies to targets which are controlled via absolute control values (= all targets with
  the exception of the "Action target" if invocation type is _Relative_).
- **Feedback transformation (EEL):** This is like _Control transformation (EEL)_ (see further below) but used for
  translating a target value back to a source value for feedback purposes. It usually makes most
  sense if it's exactly the reverse of the control transformation. Be aware: Here `x` is the desired
  source value (= output value) and `y` is the current target value (= input value), so you must
  assign the desired source value to `x`. Example: `x = y * 2`. ReaLearn's feedback processing order is like this
  (ReaLearn versions < 1.12.0 contained a bug that caused step 2 and 3 to be swapped):
  1. Apply reverse
  2. Apply target interval
  3. Apply transformation
  4. Apply source interval
  
##### For all source characters (but encoders feedback only)

The following elements are relevant for all kinds of sources. For rotary encoders they are relevant only in
*feedback* direction, not in *control* direction.

- **Source Min/Max:** The observed range of absolute source control values. By restricting that
  range, you basically tell ReaLearn to react only to a sub range of a control element, e.g. only
  the upper half of a fader or only the lower velocity layer of a key press. In relative mode, this
  only has an effect on absolute source control values, not on relative ones. This range also 
  determines the minimum and maximum feedback value.
- **Reverse:** If checked, this inverses the direction of the change. E.g. the target value will
  decrease when moving the fader upward and increase when moving it downward.
- **Out-of-range behavior:** This determines ReaLearn's behavior if the source value is not within
  "Source Min/Max" or the target value not within "Target Min/Max". There are these variants:
  
  |                | Control direction (absolute mode only)                                                                                                                                                                                                       | Feedback direction                                                                                                                                                                                                                 |
  |----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
  | **Min or max** | If the source value is < "Source Min", ReaLearn will behave as if "Source Min" was received (or 0% if min = max).<br><br>If the source value is > "Source Max", ReaLearn will behave as if "Source Max" was received (or 100% if min = max). | If the target value is < "Target Min", ReaLearn will behave as if "Target Min" was detected (or 0% if min = max).<br><br>If the target value is > "Target Max", ReaLearn will behave as if "Target Max" was detected (or 100% if min = max). |
  | **Min**        | ReaLearn will behave as if "Source Min" was received (or 0% if min = max).                                                                                                                                                                   | ReaLearn will behave as if "Target Min" was detected (or 0% if min = max).                                                                                                                                                                   |
  | **Ignore**     | Target value won't be touched.                                                                                                                                                                                                               | No feedback will be sent.                                                                                                                                                                                                                    |

##### For knobs/faders and buttons (control only)

These are elements which are relevant only for sources that emit absolute values - knobs, faders, buttons etc.
They don't apply to rotary encoders for example. They don't affect *feedback*. 

- **Mode:** Let's you choose an *absolute mode*, that is, the way incoming absolute source values are handled.
  - **Normal:** Takes and optionally transforms absolute source control values *the normal way*. _Normal_ means that
    the current target value is irrelevant and the target will just be set to whatever absolute control value is
    coming in (potentially transformed).
  - **Incremental buttons:** With this you can "go relative" without having encoders, provided your control elements
    are buttons. Let's assume you use the _MIDI Note velocity_ and select *Incremental buttons* mode. 
    Then it works like this: Each time you press the key, the target value will increase, according to the mode's
    settings. You can even make the amount of change velocity-sensitive! If you want the target value to decrease,
    just check the _Reverse_ checkbox. 
  - **Toggle:** Toggle mode is a very simple mode that takes and optionally transforms absolute source control
    values. It's used to toggle a target between _on_ and _off_ states. Only makes sense for button-like
    control elements.
    - **Important:** Sometimes the controller itself provides a toggle mode for buttons. Don't use this!
    Always set up your controller buttons to work in momentary mode! It's impossible for the controller
    to know which state (on/off) a target currently has. Therefore, if you use the controller's built-in
    toggle function, it's quite likely that it gets out of sync with the actual target state at some point.
    ReaLearn's own toggle mode has a clear advantage here.  
- **Jump Min/Max:** If you are not using motorized faders, absolute mode is inherently prone to
  parameter jumps. A parameter jump occurs if you touch a control element (e.g. fader) whose
  position in no way reflects the current target value. This can result in audible jumps because the
  value is changed abruptly instead of continuously. _Jump Min/Max_ settings can be used to control
  jump behavior.
  - You can imagine the _Jump Max_ setting as the maximum allowed parameter jump (distance between
    two target values). By default, jumps of up to 100% are allowed, which means things can get very
    jumpy. You can set this to a very low value, e.g. 5%. Then, when you move the fader, ReaLearn
    will do absolutely nothing _until_ the fader comes very close to the current target value. This
    is called _pick up mode_ in some DAWs (what an appropriate name!). Make sure to not set _Jump
    Max_ too low, otherwise your target value might get stuck.
  - The _Jump Min_ setting is more unconventional. If you raise _Jump Min_, this effectively
    enforces parameter jumps. It's like adjusting target values to a relative grid.
- **Slowly approach if jump too big:** If you combine _Jump Max_ with enabling _Slowly approach if
  jump too big_, you gain a "Soft takeover" effect, as it is called in REAPER's built-in MIDI learn.
  In some other DAWs this is called "Scale mode". This is similar to "pick up" with the difference
  that the current target value will gradually "come your way". This results in pretty seemless
  target value adjustments but can feel weird at times because the target value can temporarily move
  in the opposite direction of the fader movement.
- **Control transformation (EEL):** This feature definitely belongs in the "expert" category. It
  allows you to write a formula that transforms incoming control values before they are passed on to
  the target. While very powerful because it allows for arbitrary transformations (velocity curves,
  random values - you name it), it's not everybody's cup of tea to write something like that. The
  formula must be written in the language [EEL2](https://www.cockos.com/EEL2/). Some REAPER power
  users might be familiar with it because REAPER's JSFX uses the same language. The most simple
  formula is `y = x`, which means there will be no transformation at all. `y = x / 2` means that
  incoming control values will be halved. You get the idea: `y` represents the desired target
  control value (= output value) and `x` the incoming source control value (= input value). Both are
  64-bit floating point numbers between 0.0 (0%) and 1.0 (100%). The script can be much more
  complicated than the mentioned examples and make use of all built-in EEL2 language features. The
  important thing is to assign the desired value to `y` at some point. Please note that the initial
  value of `y` is the current target value, so you can even "go relative" in absolute mode. ReaLearn's
  control processing order is like this:
  1. Apply source interval
  2. Apply transformation
  3. Apply target interval
  4. Apply reverse
  5. Apply rounding


##### For encoders and incremental buttons (control only)

These are elements which are relevant only for sources that emit relative values or whose values
can be converted to relative values - rotary encoders and buttons. They don't affect *feedback*.

- **Step size Min/Max:** When you deal with relative adjustments of target values in terms of
  increments/decrements, then you have great flexibility because you can influence the _amount_ of
  those increments/decrements. This is done via the _Step size_ setting, which is available for all
  _continuous_ targets.
  - _Step size Min_ specifies how much to increase/decrease the target value when an
    increment/decrement is received.
  - _Step size Max_ is used to limit the effect of encoder acceleration (in case of rotary encoders
    which support this) or changes in velocity (in case of velocity-sensitive control elements). If
    you set this to the same value like _Step size Min_, encoder acceleration or changes in velocity
    will have absolutely no effect on the incrementation/decrementation amount. If you set it to
    100%, the effect is maximized.
- **Speed Min/Max:** When you choose a discrete target, the _Step size_ label will change into
  _Speed_. _Discrete_ means there's a concrete number of possible values - it's the opposite of
  _continuous_. If a target is discrete, it cannot have arbitrarily small step sizes. It rather has
  one predefined atomic step size which never should be deceeded. So allowing arbitrary step size
  adjustment wouldn't make sense. That's why _Speed_ instead allows you to _multiply_ (positive
  numbers) or _"divide"_ (negative numbers) value increments with a factor. Negative numbers are
  most useful for rotary encoders because they will essentially lower their sensitivity. Example:
  - Let's assume you selected the discrete target _FX preset_, which is considered discrete because
    an FX with for example 5 presets has 6 well-defined possible values (including the &lt;no
    preset&gt; option), there's nothing inbetween. And let's also assume that you have a controller
    like Midi Fighter Twister whose rotary encoders don't support built-in acceleration. Now you
    slightly move an encoder clock-wise and your controller sends an increment +1. If the _Speed
    Min_ slider was at 1 (default), this will just navigate to the next preset (+1). If the _Speed
    Min_ slider was at 2, this will jump to the 2nd-next preset (+2). And so on.
  - There are FX plug-ins out there which report their parameter as discrete with an insanely small
    step size (e.g. some Native Instrument plug-ins). This kind of defeats the purpose of discrete
    parameters and one can argue that those parameters should actually be continuous. In such a case,
    moving your rotary encoder might need *a lot* of turning even if you set *Speed* to the apparent
    maximum of 100! In this case you will be happy to know that the text field next to the slider allows
    you to enter values higher than 100.
  - You can set the "Speed" slider to a negative value, e.g. -2. This is the opposite. It means you
    need to make your encoder send 2 increments in order to move to the next preset. Or -5: You need
    to make your encoder send 5 increments to move to the next preset. This is like slowing down the
    encoder movement.
- **Rotate:** If unchecked, the target value will not change anymore if there's an incoming
  decrement but the target already reached its minimum value. If checked, the target value will jump
  to its maximum value instead. It works analogously if there's an incoming increment and the target
  already reached its maximum value.

##### For buttons (control only)

The following UI elements are relevant only for button-like sources. Also, they only affect *control*
direction.

- **Length Min/Max:** This decides how long a button needs to be pressed to have an effect.
  Obviously, this setting makes sense for button-like control elements only (keys, pads, buttons,
  ...), not for knobs or faders.
  - By default, both min and max will be at 0 ms, which means that the duration doesn't matter and
    both press (> 0%) and release (0%) will be instantly forwarded. If you change _Length Min_ to
    e.g. 1000 ms and _Length Max_ to 5000 ms, it will behave as follows:
    - If you press the control element and instantly release it, nothing will happen.
    - If you press the control element, wait for a maximum of 5 seconds and then release it, the
      control value of the press (> 0%) will be forwarded.
    - It will never forward the control value of a release (0%), so this is probably only useful for
      targets with trigger character.
  - The main use case of this setting is to assign multiple functions to one control element,
    depending on how long it has been pressed. For this, use settings like the following:
    - Short press: 0 ms - 250 ms
    - Long press: 250 ms - 5000 ms

## Automation and rendering

Similarly to control surfaces, ReaLearn is meant to be used for controlling targets "live". If you
want to _persist_ the resulting target value changes, you can do so by writing automation. Just like
with any other automation, it will be included when you render your project.

It _is_ possible to feed ReaLearn with track MIDI items instead of live MIDI data. This also results
in a kind of automation. **But be aware: This kind of "automation" will only be rendered in REAPER's
"Online Render" mode. It will be ignored when using one of the offline modes!**
