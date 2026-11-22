{
  description = "Members' door for Basecamp (LP-0001): prove NFT collection membership through Names Wallet with no account access";

  nixConfig = {
    extra-substituters = [ "https://cache.nix.logos.co/public" ];
    extra-trusted-public-keys = [ "public:l4HrXgL4nw246+LBh2SOJyhz64BoGegOYLheT/iIAPU=" ];
  };

  inputs = {
    # The builder revision Basecamp 0.3.x locks.
    logos-module-builder.url = "github:logos-co/logos-module-builder/de169fd45d7bdf6e28ce1f151b4c34880523ace9";
    # The Names Wallet module this view calls. The input name must match the dependency
    # name in metadata.json. When building outside a sibling checkout, pass
    # `--override-input names path:/abs/path/to/basecamp-module`.
    names.url = "path:../names";
    names.inputs.logos-module-builder.follows = "logos-module-builder";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosQmlModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
