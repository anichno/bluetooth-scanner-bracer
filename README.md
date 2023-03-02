# Docs
[Design Overview](docs/Design%20Overview.md)

[System Architecture](docs/System%20Architecture.md)

[Part List](docs/Part%20List.md)

[Milestones](docs/Milestones.md)

[Library Selection](docs/Library%20Selection.md)

[Links](docs/Links.md)

# Setup
- Setup vagrant (Virtualbox Provider)
	```bash
	# go grab a coffee or whatever, this will take a while
	vagrant up
	
	# get ssh-config and add it to your own
	vagrant ssh-config
	```
- Connect with VS Code, [remote-ssh](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-ssh) plugin
- Workspace will be in `/home/vagrant/project`
- Connect ESP32-S3-DevKitC, it should be auto-captured by Virtualbox

# Simulation
- Setup Wokwi VS Code Extension: https://docs.wokwi.com/vscode/getting-started
- Run 
	```bash
	makers simulate
	```
- `F1` -> Wokwi: Start Simulator
