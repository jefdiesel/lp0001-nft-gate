{
  description = "Members' door (LP-0001): a Basecamp QML view that asks Names Wallet for a membership proof";

  nixConfig = {
    extra-substituters = [ "https://cache.nix.logos.co/public" ];
    extra-trusted-public-keys = [ "public:l4HrXgL4nw246+LBh2SOJyhz64BoGegOYLheT/iIAPU=" ];
  };

  # Build from a staged copy where this app and the names module are sibling folders, or pass
  # --override-input names path:/abs/staged/names as Vest's app does for its core module.
  inputs = {
    # The builder revision Basecamp 0.3.x locks.
    logos-module-builder.url = "github:logos-co/logos-module-builder/de169fd45d7bdf6e28ce1f151b4c34880523ace9";
    # The core module this view calls. The input name must match the dependency in metadata.json.
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
