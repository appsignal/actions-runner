FROM ubuntu:latest as base

# udev is needed for booting a "real" VM, setting up the ttyS0 console properly
# kmod is needed for modprobing modules
# systemd is needed for running as PID 1 as /sbin/init
# ca-certificates, gnupg, lsb-release are needed for docker
RUN apt update && apt install -y \
		curl \
		wget \
		dbus \
		kmod \
		tar \
    util-linux \
		iproute2 \
		iputils-ping \
		net-tools \
		openssh-server \
		ca-certificates \
		gnupg \
		lsb-release \
		systemd \
		sudo \
		bash \
		udev \
		parallel \
		bridge-utils \
		iputils-ping \
		net-tools \
		locales \
		unzip xvfb libnss3-dev libgdk-pixbuf2.0-dev libgtk-3-dev libxss-dev libasound2 \
		git \
		jq \
		nano \
		libyaml-dev \
    build-essential

# Do tzdata separately to avoid interactive prompts
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y tzdata

# Update locale
RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG en_US.UTF-8
ENV LANGUAGE en_US:en
ENV LC_ALL en_US.UTF-8

# Install and enable docker
RUN mkdir -m 0755 -p /etc/apt/keyrings && \
	curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg && \
	echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
	    $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null && \
	apt update && \
	apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin && \
	apt clean && \
	rm -rf /var/lib/apt/lists/* && \
	systemctl enable docker && \
	update-alternatives --set iptables /usr/sbin/iptables-legacy

# Set the root password for logging in through the VM's ttyS0 console
RUN echo "root:root" | chpasswd

RUN groupadd user && \
	useradd -m -d /home/runner -s /bin/bash -g user -G docker runner

# Actions uses this for precompiled binaries
RUN mkdir -p /opt/hostedtoolcache \
    && chown -R runner:user /opt/hostedtoolcache \
    && chmod g+rwx /opt/hostedtoolcache

WORKDIR /home/runner

RUN GITHUB_RUNNER_VERSION=$(curl --silent "https://api.github.com/repos/actions/runner/releases/latest" | jq -r '.tag_name[1:]') \
    && curl -Ls https://github.com/actions/runner/releases/download/v${GITHUB_RUNNER_VERSION}/actions-runner-linux-x64-$GITHUB_RUNNER_VERSION.tar.gz | tar zx \
    && chown -R runner:user /home/runner

FROM base as runner

RUN wget -q -O - https://dl-ssl.google.com/linux/linux_signing_key.pub | gpg --dearmor | tee /etc/apt/trusted.gpg.d/google.gpg >/dev/null \
  && echo "deb http://dl.google.com/linux/chrome/deb/ stable main" >> /etc/apt/sources.list.d/google-chrome.list

RUN apt update && apt install -y \
		libjemalloc-dev \
		protobuf-compiler \
    libyaml-dev \
    libreadline-dev \
    ${CHROME_VERSION:-google-chrome-stable} \
  && rm /etc/apt/sources.list.d/google-chrome.list \
  && rm -rf /var/lib/apt/lists/* /var/cache/apt/*

ARG CHROME_DRIVER_VERSION
RUN if [ ! -z "$CHROME_DRIVER_VERSION" ]; \
  then CHROME_DRIVER_URL=https://edgedl.me.gvt1.com/edgedl/chrome/chrome-for-testing/$CHROME_DRIVER_VERSION/linux64/chromedriver-linux64.zip ; \
  else echo "Geting ChromeDriver latest version from https://googlechromelabs.github.io/chrome-for-testing/LATEST_RELEASE_" \
    && CHROME_MAJOR_VERSION=$(google-chrome --version | sed -E "s/.* ([0-9]+)(\.[0-9]+){3}.*/\1/") \
    && CHROME_DRIVER_VERSION=$(wget -qO- https://googlechromelabs.github.io/chrome-for-testing/LATEST_RELEASE_${CHROME_MAJOR_VERSION} | sed 's/\r$//') \
    && CHROME_DRIVER_URL=https://edgedl.me.gvt1.com/edgedl/chrome/chrome-for-testing/$CHROME_DRIVER_VERSION/linux64/chromedriver-linux64.zip ; \
  fi \
  && echo "Using ChromeDriver from: "$CHROME_DRIVER_URL \
  && echo "Using ChromeDriver version: "$CHROME_DRIVER_VERSION \
  && wget --no-verbose -O /tmp/chromedriver_linux64.zip $CHROME_DRIVER_URL \
  && rm -rf /opt/selenium/chromedriver \
  && unzip /tmp/chromedriver_linux64.zip -d /opt/selenium \
  && rm /tmp/chromedriver_linux64.zip \
  && mv /opt/selenium/chromedriver-linux64/chromedriver /opt/selenium/chromedriver-$CHROME_DRIVER_VERSION \
  && chmod 755 /opt/selenium/chromedriver-$CHROME_DRIVER_VERSION \
  && ln -fs /opt/selenium/chromedriver-$CHROME_DRIVER_VERSION /usr/bin/chromedriver

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain 1.74.0 -y
