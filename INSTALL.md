# Install Quick Look STEP

The download is for Apple silicon Macs. Finder previews, thumbnails, and installation are tested on macOS 27.2.1.

1. Open the downloaded DMG.
2. Drag **Quick Look STEP** onto the **Applications** shortcut in the disk image.
3. Open **Quick Look STEP** once from Applications.
4. Click **Enable or Disable…**. In the Extensions section of System Settings, select **By App** if needed, then open the details for **Quick Look STEP** and turn on both switches.
5. Eject the disk image. In Finder, select a `.step` or `.stp` file and press **Space**. The app can be closed while Finder previews files.

To update, open **QuickLookStep → Check for Updates…** from the app’s menu. Install the update with the same steps and replace the previous copy in Applications.

If Finder shows a generic file icon, confirm the preview extension is enabled and reopen Quick Look. You can include the resolved type from `mdls -name kMDItemContentType` when reporting a file that consistently falls back to the generic icon.

To uninstall, open Quick Look STEP and choose **Uninstall… → Show in Finder**. Move the selected app to the Trash.
