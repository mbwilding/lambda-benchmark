$ErrorActionPreference = 'Stop'
Set-PSDebug -Trace 1

cargo install --locked trunk
trunk build
