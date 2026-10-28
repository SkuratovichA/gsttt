# Records assets/impact.gif from the root of the repository: `vhs docs/demo/impact.tape`, with the gsttt to show first on PATH.
Output assets/impact.gif

Require gsttt

Set Shell "bash"
Set FontSize 17
Set Width 1280
Set Height 580
Set Padding 24
Set TypingSpeed 45ms
Set Theme "Catppuccin Mocha"

Hide
Type "source docs/demo/setup.sh fullstack && clear"
Enter
Show

Type "git ls-files"
Enter
Sleep 1.5s
Type "gsttt index"
Enter
Wait+Screen /coverage:/
Sleep 1s
Type "gsttt impact api/users.py"
Enter
Wait+Screen /fresh at/
Sleep 5s

# Remove the temporary repository the script made, out of the recording.
Hide
Type `cd / && rm -rf -- "$demo"`
Enter
Sleep 500ms
