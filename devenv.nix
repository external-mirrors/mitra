{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "stable";
  };

  services.minio = {
    enable = true;
    accessKey = "minio";
    secretKey = "minioadmin";
    region = "us-east-1";
    buckets = [ "mitra" ];
    listenAddress = "127.0.0.1:9000";
    consoleAddress = "127.0.0.1:9001";
  };

  services.postgres = {
    enable = true;
    package = pkgs.postgresql_15;
    createDatabase = true;
    initialDatabases = [
      {
        name = "mitra";
        user = "mitra";
        pass = "mitra";
      }
    ];
    listen_addresses = "127.0.0.1";
    port = 5432;
  };
}
