Vagrant.configure("2") do |config|
  config.vm.box = "ubuntu/jammy64"

  config.vm.network "private_network", ip: "192.168.33.10"

  config.ssh.private_key_path = ['~/.vagrant.d/insecure_private_key', '.vagrant/machines/default/virtualbox/private_key', '~/.ssh/id_rsa']
  config.ssh.forward_agent = true

  config.vm.provider "virtualbox" do |vb|
    # vb.gui = true
    vb.memory = 8192
    vb.cpus = 4
    vb.customize ["modifyvm", :id, "--usb", "on"]
    vb.customize ["modifyvm", :id, "--usbehci", "on"]

    # Add filter for ESP32
    vb.customize ["usbfilter", "add", "0",
    "--target", :id,
    "--name", "esp32 usb serial",
    "--productid", "ea60",
    "--vendorid", "10c4"]
	
	# Add filter for ESP32-S3 JTAG
    vb.customize ["usbfilter", "add", "0",
    "--target", :id,
    "--name", "esp32 jtag",
    "--productid", "1001",
    "--vendorid", "303A"]
  end

  config.vm.provision "shell", inline: <<-SHELL
    apt-get update
    apt-get dist-upgrade -y
    apt-get install -y build-essential libuv1-dev libudev-dev libssl-dev pkg-config linux-generic python3-pip
    usermod -a -G dialout vagrant
  SHELL

  config.vm.provision "shell", privileged: false, inline: <<-SHELL
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    cargo install espup
    espup install
    echo '. $HOME/export-esp.sh' >> $HOME/.bashrc
    cargo install --version 2.0.0-rc.3 cargo-espflash
    cargo install --version 2.0.0-rc.3 espflash
    cargo install cargo-espmonitor
    cargo install espmonitor
    cargo install cargo-generate
    cargo install ldproxy
	cargo install https
	cargo install cargo-make
    mkdir /home/vagrant/project
    cp -r /vagrant/. /home/vagrant/project
  SHELL

  config.vm.provision 'shell', reboot: true

end
