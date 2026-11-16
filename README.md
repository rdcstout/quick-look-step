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

Roger Stout maintains this independent fork at [rdcstout/quick-look-step](https://github.com/rdcstout/quick-look-step). Follow the [source build and Finder verification instructions](DEVELOPMENT.md) to try it locally.

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

## Credits and licensing

John Boiles created the original [Quick Look STEP](https://github.com/johnboiles/quick-look-step) app and identified it as MIT licensed in the original README.

This fork uses Foxtrot from Formlabs and contributors. The vendored source retains its [MIT](foxtrot/LICENSE-MIT) and [Apache 2.0](foxtrot/LICENSE-APACHE) license texts.
