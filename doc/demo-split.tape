# VHS tape file — demonstrates `squire split` workflow
# Run: vhs demo-split.tape

Output demo-split.gif

Require squire

Set Shell "bash"
Set FontSize 14
Set Width 1000
Set Height 600
Set Padding 20
Set Theme "Catppuccin Mocha"

# Hidden setup: build a repo with one commit that has two logical changes in the same file
Hide
Type "export PS1='$ '"
Enter
Type `cd $(mktemp -d) && git init demo && cd demo`
Enter
Type `git commit --allow-empty -m "init"`
Enter
Type `cat > main.rs << 'EOF'`
Enter
Type "fn main() {"
Enter
Type "}"
Enter
Type "EOF"
Enter
Type `git add main.rs && git commit -m "add main.rs"`
Enter
Type `cat > main.rs << 'EOF'`
Enter
Type "fn greet(name: &str) {"
Enter
Type `    println!("hello, {name}!");`
Enter
Type "}"
Enter
Type ""
Enter
Type "fn main() {"
Enter
Type `    greet("world");`
Enter
Type "}"
Enter
Type "EOF"
Enter
Type `git add main.rs && git commit -m "feat: add greeting and use it in main"`
Enter
Type "clear"
Enter
Show

# --- Recording starts here ---

Type "git log --oneline"
Enter
Sleep 2s

# Split the commit
Type "squire split HEAD"
Enter
Sleep 1.5s

# Show the unstaged changes — one hunk with two logical changes
Type "squire diff"
Enter
Sleep 3s

# Stage just the greet function lines by their line hashes
Type "squire stage 3342dbfd:a6-ee"
Enter
Sleep 1.5s

Type `git commit -m "feat: add greet function"`
Enter
Sleep 1s

# Show the remaining diff — new hunk ID after first commit
Type "squire diff"
Enter
Sleep 2.5s

# Stage the remaining hunk
Type "squire stage c296e916"
Enter
Sleep 1.5s

Type `git commit -m "feat: call greet from main"`
Enter
Sleep 1s

# One commit became two
Type "git log --oneline"
Enter
Sleep 3s
