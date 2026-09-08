{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
let
  inherit (pkgs.stdenv.hostPlatform) system;

  rust =
    let
      version = "1.97.0.0";
    in
    pkgs.stdenv.mkDerivation {
      pname = "rust";
      inherit version;
      src = pkgs.fetchzip {
        name = "rust-${version}";
        url =
          let
            system' =
              {
                x86_64-linux = "x86_64-unknown-linux-gnu";
                aarch64-linux = "aarch64-unknown-linux-gnu";
                aarch64-darwin = "aarch64-apple-darwin";
              }
              .${system};
          in
          "https://github.com/esp-rs/rust-build/releases/download/v${version}/rust-${version}-${system'}.tar.xz";
        hash =
          {
            x86_64-linux = "sha256-CnxysCl+Hcllge0olHSBOstgbhqDsCRAhg+mDUAnEs0=";
            aarch64-linux = "sha256-XPHmkPk/qaR3B1inR7d9zrACJDlfik4CbMj9BYsrN+g=";
            aarch64-darwin = "sha256-iRTnPWFARS70lEJi6szcnVlTFCkzYPhdnpkB50I+Wu8=";
          }
          .${system};
      };
      patchPhase = "patchShebangs .";
      nativeBuildInputs =
        with pkgs;
        [ makeWrapper ]
        ++ lib.optionals stdenv.hostPlatform.isLinux [
          autoPatchelfHook
          pkg-config
        ]
        ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.autoSignDarwinBinariesHook ];
      buildInputs =
        with pkgs;
        lib.optionals stdenv.hostPlatform.isLinux [
          stdenv.cc.cc
          zlib
        ];
      # Let’s only install the `rustc` component (and `rust-std-x86_64-unknown-linux-gnu` for `build.rs` scripts):
      installPhase = ''
        mkdir -p $out
        ./install.sh --destdir=$out --prefix= --disable-ldconfig \
          --without=cargo,rustfmt-preview,clippy-preview,rust-docs,rust-docs-json-preview
        chmod -R +w $out
        ln -s ${rust-src}/lib/rustlib/src $out/lib/rustlib/src

        for exe in $out/bin/{rustc,rustdoc} ; do
          wrapProgram "$exe" --prefix PATH : ${
            lib.makeBinPath [
              esp-gcc.xtensa
              esp-gcc.riscv32
              pkgs.stdenv.cc # needed for `build.rs` scripts which run on the host
            ]
          }
        done
      '';
      dontStrip = pkgs.stdenv.hostPlatform.isDarwin; # or else no `.rmeta` section in `…/libcore-….rlib` etc.
      meta.mainProgram = "rustc";
    };

  rust-src =
    let
      inherit (rust) version;
    in
    pkgs.stdenv.mkDerivation {
      pname = "rust-src";
      inherit version;
      src = pkgs.fetchzip {
        name = "rust-src-${version}";
        url = "https://github.com/esp-rs/rust-build/releases/download/v${version}/rust-src-${version}.tar.xz";
        hash = "sha256-0Q/hfXD0R1pVzTgS0N8X2D3ZvPYX4y4k/EmxPy2474k=";
      };
      patchPhase = "patchShebangs .";
      installPhase = ''
        mkdir -p $out
        ./install.sh --destdir=$out --prefix= --disable-ldconfig
      '';
    };

  esp-gcc =
    let
      version = "16.1.0_20260609";
    in
    lib.genAttrs [ "xtensa" "riscv32" ] (
      target:
      pkgs.stdenv.mkDerivation {
        pname = "esp-gcc-${target}";
        inherit version;
        src = pkgs.fetchzip {
          name = "esp-gcc-${target}-${version}";
          url =
            let
              system' =
                {
                  x86_64-linux = "x86_64-linux-gnu";
                  aarch64-linux = "aarch64-linux-gnu";
                  aarch64-darwin = "aarch64-apple-darwin";
                }
                .${system};
            in
            "https://github.com/espressif/crosstool-NG/releases/download/esp-${version}/${target}-esp-elf-${version}-${system'}.tar.xz";
          hash =
            {
              x86_64-linux = {
                xtensa = "sha256-D02nz89injwvi+CD8tE8j/xkp1YrS28XvAmqCd3Dm+A=";
                riscv32 = "sha256-XmWOvWlgKYgZHpquEx3qUVjkwfYrpGZF386GfVDWdAA=";
              };
              aarch64-linux = {
                xtensa = "sha256-SL3wIxnkcYJw04A9J1GTmpLvlE1iby5HdtLYmFwRaps=";
                riscv32 = "sha256-GKn2MGsSfY8ZNrq7KFM1nPo+ChK2dcNj3pyIMtaPDvY=";
              };
              aarch64-darwin = {
                xtensa = "sha256-O0gXFHa127y5hzwRJeXcvs3ZtF2eK93YJcwut9P9gok=";
                riscv32 = "sha256-ui6SL84mAXNOS9np+lQpJH4QqF9wTL86zyWwm7vv3NY=";
              };
            }
            .${system}.${target};
        };
        patchPhase = "patchShebangs .";
        nativeBuildInputs =
          with pkgs;
          lib.optionals stdenv.hostPlatform.isLinux [
            autoPatchelfHook
            pkg-config
          ]
          ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.autoSignDarwinBinariesHook ];
        buildInputs =
          with pkgs;
          lib.optionals stdenv.hostPlatform.isLinux [
            stdenv.cc.cc
            zlib
          ];
        installPhase = "cp -r . $out";
      }
    );

  esp-gdb =
    let
      version = "17.1_20260402";
      python = pkgs.python3;
      pythonVersion = lib.concatStringsSep "." (lib.take 2 (lib.splitVersion python.version));
    in
    lib.genAttrs [ "xtensa" "riscv32" ] (
      target:
      pkgs.stdenv.mkDerivation {
        pname = "esp-gdb-${target}";
        inherit version;
        src = pkgs.fetchzip {
          name = "esp-gdb-${target}-${version}";
          url =
            let
              system' =
                {
                  x86_64-linux = "x86_64-linux-gnu";
                  aarch64-linux = "aarch64-linux-gnu";
                  aarch64-darwin = "aarch64-apple-darwin24.5";
                }
                .${system};
            in
            "https://github.com/espressif/binutils-gdb/releases/download/esp-gdb-v${version}/${target}-esp-elf-gdb-${version}-${system'}.tar.gz";
          hash =
            {
              x86_64-linux = {
                xtensa = "sha256-wnlCFiWYEsewjhx07ZXTzVn8/e67zACTafNAwlzcG/I=";
                riscv32 = "sha256-Ldh/Qrxw5fbxhYDYZXqLuU+Jn40cR5VwuTxwPCpTYPs=";
              };
              aarch64-linux = {
                xtensa = "sha256-OEQmdd2urX7f2vDMlLFKJ3WQjT3TJCcOusarYneSsYE=";
                riscv32 = "sha256-O7uGkX89KUv2YEB7UA2rt9FmU8L4J7bvKcNxorFVIPg=";
              };
              aarch64-darwin = {
                xtensa = "sha256-0Gx6SquQyPQobJNxSlaUHWQZq+vBWJx3ahSACpFu/50=";
                riscv32 = "sha256-T+2utYRNOH112pJtBMBeW32so6jsGNG1eTJ4TwarcIk=";
              };
            }
            .${system}.${target};
        };
        patchPhase = "patchShebangs .";
        nativeBuildInputs =
          with pkgs;
          lib.optionals stdenv.hostPlatform.isLinux [
            autoPatchelfHook
            pkg-config
          ]
          ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.autoSignDarwinBinariesHook ];
        buildInputs =
          with pkgs;
          lib.optionals stdenv.hostPlatform.isLinux [
            stdenv.cc.cc
            zlib
            python3
          ];
        installPhase = ''
          cp -r . $out
          chmod -R +w $out
          cd $out/bin
          ls ${target}-esp-elf-gdb-3.* | grep -vF ${pythonVersion} | xargs rm
        '';
      }
    );
in
{
  env.RUSTC = lib.getExe rust;
  env.RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
  env.ESPFLASH_SKIP_UPDATE_CHECK = "true";

  packages = [
    rust-src
    rust
    esp-gcc.xtensa
    esp-gcc.riscv32
    esp-gdb.xtensa
    esp-gdb.riscv32

    pkgs.cargo

    pkgs.esp-generate
    pkgs.espflash
  ];
}
