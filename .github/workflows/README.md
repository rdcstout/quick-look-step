# GitHub Actions

The [build workflow](build.yml) uses a read-only job for ordinary source builds and tests. Release-tag pushes also run a separate `release` job that uses a Developer ID Application certificate to sign the app, notarizes the archive with Apple, validates the stapled signature, and attaches the final ZIP to the GitHub release.

Configure the `release` environment before pushing a release tag. Set the `MACOS_DEVELOPMENT_TEAM` variable to the Developer ID team's ID. Restrict the environment to approved release activity and keep credentials in environment secrets.

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded Developer ID Application certificate and private key exported as a password-protected `.p12` |
| `MACOS_CERTIFICATE_PASSWORD` | Password for the `.p12` export |
| `APPSTORE_CONNECT_API_KEY_ID` | Notarization API key ID |
| `APPSTORE_CONNECT_API_ISSUER_ID` | App Store Connect issuer ID |
| `APPSTORE_CONNECT_API_PRIVATE_KEY` | Contents of the notarization key's `.p8` file |

See [Development](../../DEVELOPMENT.md) for local builds and Finder verification.
