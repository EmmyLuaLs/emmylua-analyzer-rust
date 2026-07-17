let
  root = ../.;
  mkPackage =
    x:
    {
      rustPlatform,
      pkg-config,
      openssl,
    }:
    let
      cargoToml = builtins.fromTOML (builtins.readFile /${root}/crates/${x}/Cargo.toml);
    in
    rustPlatform.buildRustPackage {
      pname = cargoToml.package.name;
      version = cargoToml.package.version;

      strictDeps = true;

      src = root;
      cargoLock.lockFile = root + /Cargo.lock;

      nativeBuildInputs = [ pkg-config ];
      buildInputs = [ openssl ];

      # Needed to get openssl-sys to use pkg-config.
      env.OPENSSL_NO_VENDOR = 1;

      buildAndTestSubdir = "crates/${x}";
    };
in
(builtins.listToAttrs (
  map
    (
      x:
      let
        name = "emmylua_${x}";
      in
      {
        inherit name;
        value = mkPackage name;
      }
    )
    [
      "ls"
      "doc_cli"
      "check"
    ]
))
