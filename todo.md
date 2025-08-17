- [x] : There's way too many helper processes caught in my filters
- [ ] : Add killlist and manual process selection
- [ ] : Hotkey
- [ ] : find what makes figma unkillable (or rather ... undead? ...)
- [ ] : make it tray 
- [ ] : and startup
- [ ] : installer because hell
- [ ] : handle chrome profiles separately
- [ ] : make it work on linux???
- [ ] : actual ui you know
- [ ] : search for processes and allowlist
- [ ] : handle explorer specially

Main optimizations:
Using Rust because it's fast, low-level and efficient and cool. Written cleanly it saves a lot of memory.
Using the most appropriate data types (e.g. HashSet instead of Vec) to save on memory
Using borrowing instead of cloning everything
You could also turn off GPU computing for an extra free 20mb of RAM ??? turn on?

Optimization techniques and architecture decisions:
Before anything, choosing the correct library and framework for the app would be the biggest memory saver. I was having a hard time picking between Iced, Egui and Slint but then i found the [repo](https://github.com/maurges/every-rust-gui-library) of a guy who tried them all, how convenient. I also figured Egui runs every frame so it doesn't store state thus it'll be more memory efficient so I went with it. Now don't get me wrong, I could go for something lower level like Winit idk, but my Rust is mediocre and I never made an app with it (except like Tauri but that doesn't count), so cut me some slack.
~~Soon I realized that the least memory-efficient part of my app is the gpu drawing and that getting it under 75 mb wasn't even a challenge at all so I was just having fun instead of actually optimizing even if I did try to write clean code.~~
Sets, B-trees, &str and other datatypes are things I knew from other languages already and well Rust itself, so utilizing them where possible is exactly what I did. I think.