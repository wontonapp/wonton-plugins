// 上游约定：/etc/scanservjs/config.local.js 只在启动时读，改了要重启插件。
// 云吞把“文档”共享文件夹整个挂载在 /documents，扫描结果指到它下面的子目录，
// preview/temp/thumbnail 仍留在插件数据目录。
module.exports = {
  afterConfig(config) {
    config.outputDirectory = '/documents/扫描件';
  },
};
