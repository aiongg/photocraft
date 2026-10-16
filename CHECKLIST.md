# PhotoCraft Mac test — `next` 121765dd5

Report results by number ("1–9 pass, 14 skipped, 21 fails: …"). Items 1–13 are the context-menu scroll arrows, Image Size on Indexed Color, Window › Arrange and the filter previews: test those first.

## 1. Build (give this to the Mac agent)

```
In ~/aiongg/photocraft, build the PhotoCraft test app from the fork's `next` branch. Don't launch the app.
1. git fetch origin && git checkout next && git reset --hard origin/next; confirm HEAD is 121765dd5.
2. CRAFT_FONTS_DIR=$HOME/aiongg/craft-fonts packaging/macos/package.sh --arch aarch64
3. mkdir -p ~/aiongg/photocraft-test && /bin/cp -Rf target/macos-package/PhotoCraft.app target/macos-package/photocraft-cli-*-macos-aarch64/photocraft-cli ~/aiongg/photocraft-test/
4. /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f ~/aiongg/photocraft-test/PhotoCraft.app
Report the commit you built and whether each step succeeded.
```

Then open `~/aiongg/photocraft-test/PhotoCraft.app`.

## 2. Checks

### Context menus — #6417
- [ ] 1. Drag the window's bottom edge up to about half the screen. Right-click a layer in Layers (F7): the menu stays inside the window, with a ▲ row at its top and a ▼ row at its bottom.
- [ ] 2. Hover ▼: the menu scrolls down and stops at the last item, where ▼ dims. Hover ▲: it scrolls back up. Click an arrow: it jumps several rows.
- [ ] 3. Scroll to the bottom and choose the last item: it runs. Make the window tall again: when the layer menu fits, it has no arrow rows.

### Image Size and File › Open — #6035, #6346
- [ ] 4. Open a photo, Image › Mode › Indexed Color (OK the defaults), then Image › Image Size… (⌥⌘I): the Resample list offers only Automatic and Nearest Neighbor (hard edges).
- [ ] 5. Width 400 %, OK, View › 100% (⌘1): edges are hard pixel steps with no new blended colours. Save As a GIF, close, reopen: it looks as it did before the save.
- [ ] 6. Image › Mode › RGB Color, then Image Size again: the full Resample list (Bicubic, Bilinear, Lanczos, Preserve Details …) is back.
- [ ] 7. File › Open… (⌘O): PSD, JPEG, TIFF, Targa, Paint.NET (.pdn) and .abr brush files are all selectable; a .txt is greyed. (The Mac panel has no per-format menu.)

### Window › Arrange — #6039
- [ ] 8. Open three documents, Window › Arrange › Tile All Vertically, click into the right-hand tile: the tiles keep their places and that document becomes active where it is. A brush stroke lands in it.
- [ ] 9. View › Rulers (⌘R): every tile shows rulers for its own document. Drag a guide out of the active tile's ruler: it lands in that document.
- [ ] 10. Window › Arrange › 2-up Vertical with three documents open, activate the third from the Window menu: the tiles slide to show it, and the other two don't swap.

### Filter previews — #6351, #6338
- [ ] 11. A photo at View › 100% (⌘1), Filter › Blur › Gaussian Blur…, Preview on, change Radius: the canvas preview is sharp at the pixel level, never blocky. OK: the result matches the preview.
- [ ] 12. Filter › Convert for Smart Filters, then Gaussian Blur at radius 5, OK. Double-click "Gaussian Blur" under the layer in Layers: the dialog has its Preview box, and a new radius updates the canvas.
- [ ] 13. OK: Layers lists one Gaussian Blur with the new radius, not two. Reopen, change the radius, Cancel: the image is back at the previous radius.

### Help › Search — #6030
- [ ] 14. Quit, then in Terminal: `open ~/aiongg/photocraft-test/PhotoCraft.app --args --in-window-menus`. Click Help in the window's menu bar, type "Deselect": the field holds the whole word, Help stays open, nothing else opens. Quit and open normally afterwards.

### Tools and selection — #6364, #6503
- [ ] 15. Pen tool (P): draw a closed path. Path Selection tool (A), right-click the canvas: the path menu opens (Delete Path, Make Selection…, Fill Path…, Stroke Path…, Free Transform Path). Make Selection…, OK: a selection follows the path.
- [ ] 16. Direct Selection (⇧A cycles to it), right-click: the same menu. Right-click the pasteboard outside the image with either tool: the same path menu, not the pasteboard menu.
- [ ] 17. Crop tool (C), preset dropdown Ratio, type 1.414 and 1 in the two fields, click elsewhere: the field still reads 1.414. ⇄ (or X): the fields read 1 and 1.414.
- [ ] 18. Preset dropdown › New Crop Preset…: the proposed Name is "1.414 : 1". A 16 : 9 ratio still reads "16 : 9".

### Type — #6366, #6038, #6453, #6505
- [ ] 19. Type tool (T), click into a white area: the caret is a black line with a white edge, easy to see, and blinks. On a black area it is visible too.
- [ ] 20. Drag a paragraph text box about 5 cm wide, paste `여기서는 각종 설정이나 캐릭터 제작을 진행합니다.`: lines break only at spaces; "설정이나" and "제작을" stay whole. Japanese text still wraps between characters.
- [ ] 21. Type tool options bar: hover the "T" icon left of the size field: tooltip "Font size". Drag it right: the size grows; ⇧ takes bigger steps. One ⌘Z undoes the whole drag.
- [ ] 22. While editing text, select a few characters and drag the "T": only those characters change size.
- [ ] 23. PhotoCraft › Settings › Units & Rulers…, Type: Pixels. On a 144 ppi document with 12 pt text: Window › Character shows Size 24 px, leading and baseline shift in px; Window › Paragraph shows indents and spacing in px.
- [ ] 24. Type 36 in Character's size: the text becomes 36 px. Set Type back to Points: the field reads 18 pt.

### Colour: Replace Color — #6335, #6344
- [ ] 25. Image › Adjustments › Replace Color…: the eyedropper is already on, and the first click on the image picks the From colour. The eyedropper button still turns it off and on.
- [ ] 26. Click the Result swatch: the Color Picker opens. Pick another colour, OK: Hue, Saturation and Lightness move so that Result shows that colour, and the image follows.
- [ ] 27. Click Result again, then Cancel (or OK with no change): the sliders stay put. Move a slider after a pick: it stays where you put it.

### Brushes — #6399
- [ ] 28. Brush tool (B), Window › Brush Settings (F5), tick Dual Brush, set its Size to 20 px with the main size at 100 px. Set the options bar size to 1, then 100: Dual Brush Size reads 20 px again.
- [ ] 29. Paint at 1 px and at 100 px with that brush: no crash, and both strokes show the dual tip.

### Formats: Targa — #6386
- [ ] 30. A photo or screenshot with flat areas, File › Save As… (⇧⌘S) as `test.tga`, 24 bits. Open it in Preview: it matches PhotoCraft, with no shifted or skewed rows. Reopen in PhotoCraft: the same.
- [ ] 31. A layer with transparent areas, saved as Targa at 32 bits: Preview and PhotoCraft both show the transparency.

### Launch flags and logging — #6499, #6478
- [ ] 32. Quit. In Terminal: `~/aiongg/photocraft-test/PhotoCraft.app/Contents/MacOS/PhotoCraft --control; echo $?`: it prints "photocraft: --control: missing port value …", then 2, and no window opens.
- [ ] 33. Open PhotoCraft, then in Terminal run the same binary with an image path: the image opens in the running app. In `~/Library/Application Support/Photocraft/logs`, photocraft.log still holds the running app's log, and a small photocraft-<pid>.log sits beside it.

### Also note
- Any crash, freeze or spinning beachball, and what you did just before it.
- Not testable on the Mac: #6412 (Windows recovery folder), #6394 (Linux AppImage launcher), #6422 (Linux file dialog portal), #6414 (web download and its close prompt).
- Non-UI changes with no Mac check: #6476, #6510 (automation token case, root aliases), #6502 (engine number overflow), #6506 (Affinity nested spot colours), #6485 (CSV data set BOM), #5992 (retouch stroke bounds), #5671 (SVG raster budget).
