# GitHub Actions

The [build workflow](build.yml) uses a read-only job for source builds and tests. A manual run with the packaging option enabled builds a Developer ID signed, notarized candidate ZIP and uploads it as a workflow artifact for installation testing.

Configure the `release` environment before requesting a signed candidate. Set the `MACOS_DEVELOPMENT_TEAM` variable to the Developer ID team's ID. Restrict the environment to approved release activity and keep credentials in environment secrets.

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded Developer ID Application certificate and private key exported as a password-protected `.p12` |
| `MACOS_CERTIFICATE_PASSWORD` | Password for the `.p12` export |
| `APPSTORE_CONNECT_API_KEY_ID` | Notarization API key ID |
| `APPSTORE_CONNECT_API_ISSUER_ID` | App Store Connect issuer ID |
| `APPSTORE_CONNECT_API_PRIVATE_KEY` | Contents of the notarization key's `.p8` file |

Publish the exact ZIP tested through the documented installation path. Upload the versioned ZIP and a byte-identical `QuickLookStep-macOS-arm64.zip` copy to the GitHub release. Creating a tag does not build or publish a package.

See [Development](../../DEVELOPMENT.md) for local builds and Finder verification.
