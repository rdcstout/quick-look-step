# QuickLookStep.app

> **_Fast_** QuickLook & Finder thumbnail support for STEP (`.step` / `.stp`) 3d model files on macOS.
>
> Built with Swift + SwiftUI, SceneKit, and [Formlabs/foxtrot](https://github.com/Formlabs/foxtrot).

## ✨ What does it do?

This allows you to **quickly** preview STEP files from Finder before opening them in your CAD tool. It will not render your STEP files as well as your CAD tool, but it will almost certainly open them faster!

https://github.com/user-attachments/assets/339781d5-3b7b-41c0-b411-d992e49ae5bc

## 🚧 Requirements

* macOS 14.6 (Sonoma) or newer
* M1 or above CPU (Apple Silicon)

## 💻 Building the fork

This fork is under development. Follow the [source build and Finder verification instructions](DEVELOPMENT.md) to try it locally. The signed v1.6 download and Homebrew tap belong to the upstream project.

## 🕶️ Usage

Open a Finder window and select a `.step` or `.stp` file. You should be able to preview it from Finder with Quick Look (using spacebar) or see the thumbnail in the sidebar in column view. `QuickLookStep.app` does _not_ need to be open for the extensions to work.

## 🐞 Known issues

* Some STEP files don't load. The underlying [Foxtrot](https://github.com/Formlabs/foxtrot) library doesn't like them.
* Some STEP files have holes. Probably this is also a [Foxtrot](https://github.com/Formlabs/foxtrot) thing, but I could also be making a mistake in how I'm loading geometry to SceneKit. More testing against [Foxtrot](https://github.com/Formlabs/foxtrot) is needed. For this app, it's always better to be fast than perfect, but ideally it can be both 😀
* The examples in the [Foxtrot README.md](https://github.com/Formlabs/foxtrot/blob/master/README.md) suggests Foxtrot supports textures. I don't have that hooked up in SceneKit yet.
* Very large assemblies can take a long time to load and the extensions get stuck for a bit. Better timeouts could probably protect against this.

## 🔫 Troubleshooting

See [Finder verification](DEVELOPMENT.md#finder-verification) for extension registration and STEP file-type checks.

## 🤝 Contributing

See [Contributing](CONTRIBUTING.md) for bug reports and changes.

This project is licensed under the terms of the **MIT License**. Do what you want with it but I don't guarantee it works. If you do something neat with it, I'd always appreciate a shoutout!
