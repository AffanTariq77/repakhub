# Code signing

Unsigned installers work, but Windows SmartScreen and macOS Gatekeeper warn before they open. Signing removes the macOS warning entirely and, on Windows, ties the installer to a verified publisher. This is separate from the updater key (`docs/architecture.md`, "RepakHub's own updates"), which RepakHub uses to verify its own updates whether or not the installers are signed.

## macOS: wired, needs an Apple Developer account

The release workflow signs with a Developer ID Application certificate and notarises with Apple as soon as the secrets below exist. Tauri does the work during `tauri build`: it imports the certificate into a temporary keychain, signs the app, submits it to Apple's notary service and staples the ticket. A check step then runs `codesign --verify`, `stapler validate` and `spctl --assess`, so a release whose app Gatekeeper would still block fails instead of publishing. Without the secrets, macOS builds stay unsigned as before.

One-time setup (Apple Developer Program, USD 99 a year):

1. In Xcode (Settings, Accounts, Manage Certificates) or at developer.apple.com, create a **Developer ID Application** certificate. Export it from Keychain Access as a `.p12` with a strong password.
2. At appleid.apple.com, create an app-specific password for notarisation.
3. Find your Team ID at developer.apple.com, Membership.
4. From the repository folder, set the secrets (files are read directly, so nothing is shown on screen):

   ```
   base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD   # the .p12 password, typed at the prompt
   gh secret set APPLE_ID                     # your Apple ID email
   gh secret set APPLE_PASSWORD               # the app-specific password
   gh secret set APPLE_TEAM_ID                # e.g. ABCDE12345
   ```

   Optional: `APPLE_SIGNING_IDENTITY` (for example `Developer ID Application: Your Name (ABCDE12345)`). Tauri reads the identity from the certificate when it is not set.

5. Delete the exported `.p12` from disk, or keep it with the other keys in `~/.repakhub-keys/`.

Trade-off: `codesign` only runs on macOS, so the Apple secrets go to the macOS build step, which also runs the project's build scripts and dependencies. A compromised build dependency could misuse the certificate there; the updater key and the release token stay in the separate `publish` job. If that certificate is ever exposed, revoke it at developer.apple.com.

## Windows: not wired, needs a certificate provider

Since 2023, publicly trusted code-signing keys must live on hardware, so signing in CI goes through a cloud signing service. The options for this project:

| Option | Fits? |
| --- | --- |
| Azure Artifact Signing (formerly Trusted Signing), the cheapest | Not available: individual developers must be in the United States or Canada |
| SignPath Foundation, free for open-source projects | Only after the repository has an OSI-approved licence, which it does not have yet; the certificate is issued in SignPath Foundation's name |
| A standard (OV) certificate from a CA with cloud signing, such as SSL.com eSigner or Certum | Works from anywhere; paid yearly, with identity validation |

An EV certificate no longer skips SmartScreen's reputation check, so an OV certificate is enough. SmartScreen warnings fade as signed downloads build reputation.

Once a provider is chosen, signing goes in the Windows build through Tauri's `bundle.windows.signCommand`, which runs the provider's tool on each file (the MSI, the NSIS installer and the app itself), with the provider's credentials as secrets given only to that step.
