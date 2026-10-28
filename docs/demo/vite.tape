# Records assets/vite.gif from the root of the repository: `vhs docs/demo/vite.tape`, with the gsttt to show first on PATH. It fetches vite, so it needs the network.
Output assets/vite.gif

Require gsttt
Require git

Set Shell "bash"
Set FontSize 17
Set Width 1280
Set Height 700
Set Padding 24
Set TypingSpeed 45ms
Set Theme "Catppuccin Mocha"
Set WaitTimeout 120s

Hide
Type "source docs/demo/vite.sh && clear"
Enter
Wait+Line /^\$$/
Show

Type "gsttt index"
Enter
Wait+Screen /coverage:/
Sleep 1.5s
Type "echo '// one more line' >> packages/plugin-legacy/src/types.ts"
Enter
Sleep 0.5s
Type "gsttt index"
Enter
Wait+Screen /1 parsed/
Sleep 1.5s
Type "gsttt impact --changed"
Enter
Wait+Screen /fresh at/
Sleep 6s

# Remove the temporary repository the script made, out of the recording.
Hide
Type `cd / && rm -rf -- "$demo"`
Enter
Sleep 500ms
