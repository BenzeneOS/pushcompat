flake:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib)
    mkEnableOption
    mkOption
    types
    mkIf
    literalExpression
    ;

  cfg = config.services.pushcompat-bridge;
in
{
  options.services.pushcompat-bridge = {
    enable = mkEnableOption "pushcompat-bridge FCM to UnifiedPush relay server";

    package = mkOption {
      type = types.package;
      default = flake.packages.${pkgs.stdenv.hostPlatform.system}.pushcompat-bridge;
      defaultText = literalExpression "flake.packages.\${pkgs.stdenv.hostPlatform.system}.pushcompat-bridge";
      description = "The pushcompat-bridge package to use.";
    };

    port = mkOption {
      type = types.port;
      default = 8080;
      description = "HTTP server port for registration API.";
    };

    endpointHosts = mkOption {
      type = types.listOf types.str;
      default = [ ];
      example = [ "ntfy.amaanq.com" ];
      description = ''
        Allowlist of UP endpoint hosts (exact match or subdomain).
        Empty means any https endpoint is accepted.
      '';
    };

    publicOrigin = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "https://push.benzeneos.org";
      description = ''
        Public origin required as the VAPID `aud` claim. VAPID enforcement is
        disabled while this is null, so web push endpoints stay bearer
        credentials.
      '';
    };

    vapidKillFile = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "/var/lib/pushcompat-bridge/vapid-off";
      description = ''
        VAPID enforcement is skipped while this file exists. Must be somewhere
        the service can traverse, or it never fires.
      '';
    };

    logLevel = mkOption {
      type = types.enum [
        "off"
        "error"
        "warn"
        "info"
        "debug"
        "trace"
      ];
      default = "info";
      description = "Maximum bridge log level.";
    };

    stateDir = mkOption {
      type = types.str;
      default = "/var/lib/pushcompat-bridge";
      description = "Directory to store state (SQLite database).";
    };

    user = mkOption {
      type = types.str;
      default = "pushcompat-bridge";
      description = "User to run pushcompat-bridge as.";
    };

    group = mkOption {
      type = types.str;
      default = "pushcompat-bridge";
      description = "Group to run pushcompat-bridge as.";
    };
  };

  config = mkIf cfg.enable {
    users.users.${cfg.user} = {
      inherit (cfg) group;
      isSystemUser = true;
      description = "pushcompat-bridge service user";
      home = cfg.stateDir;
    };

    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.pushcompat-bridge = {
      description = "FCM to UnifiedPush relay server";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.stateDir;
        Restart = "on-failure";
        RestartSec = "10s";

        # Hardening
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        RestrictNamespaces = true;
        ReadWritePaths = [ cfg.stateDir ];
      };

      script =
        let
          arguments = [
            "--port"
            (toString cfg.port)
            "--db-path"
            "${cfg.stateDir}/pushcompat.db"
            "--log-level"
            cfg.logLevel
          ]
          ++ lib.concatMap (host: [ "--endpoint-host" host ]) cfg.endpointHosts
          ++ lib.optionals (cfg.publicOrigin != null) [ "--public-origin" cfg.publicOrigin ]
          ++ lib.optionals (cfg.vapidKillFile != null) [ "--vapid-kill-file" cfg.vapidKillFile ];
        in
        ''
          exec ${lib.getExe cfg.package} ${lib.escapeShellArgs arguments}
        '';
    };
  };
}
