# Records docs/public/img/demo.gif with VHS in CI (.github/workflows/demo.yml).
Output docs/public/img/demo.gif

Require lockdocs

Set Shell "bash"
Set FontSize 19
Set Width 1400
Set Height 780
Set Padding 26
Set Theme "Catppuccin Mocha"
Set TypingSpeed 38ms
Set WindowBar Colorful

Hide
Type "export PS1='\[\e[38;5;111m\]\W\[\e[0m\] $ ' && cd bench/projects/next14 && clear"
Enter
Show

Type "# Is cookies() async? Depends on your Next.js."
Enter
Sleep 600ms
Type "lockdocs api cookies --pkg next | head -5"
Enter
Sleep 2.8s
Type "cd ../next15 && lockdocs api cookies --pkg next | head -5"
Enter
Sleep 3.2s
Type "clear"
Enter
Type "# Same question, pydantic 1 vs pydantic 2"
Enter
Sleep 600ms
Type "cd ../pydantic1 && lockdocs pydantic 'convert a model to a dict' | sed -n 3,8p"
Enter
Sleep 3.5s
Type "cd ../pydantic2 && lockdocs pydantic 'convert a model to a dict' | sed -n 3,8p"
Enter
Sleep 4.5s
