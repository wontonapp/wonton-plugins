# 云吞插件：Ollama

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

在云吞里跑 Ollama，用本机算力跑本地大模型。模型经模型网关（magpie）给所有应用和 Bot 用：装好后打开“模型设置”，把
`http://<主机的局域网地址>:11434` 填成 Ollama 的 provider；局域网里的电脑和手机也能直接连这个地址。有显卡就自动用显卡，
没有就退回 CPU。
