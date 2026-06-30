#pragma once

#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <fcitx/addoninstance.h>
#include <fcitx/instance.h>

namespace vinput_fcitx_bridge {

class FcitxVinputAddon final : public fcitx::AddonInstance {
public:
  explicit FcitxVinputAddon(fcitx::Instance *instance);
  ~FcitxVinputAddon() override = default;

  FcitxVinputAddon(const FcitxVinputAddon &) = delete;
  FcitxVinputAddon &operator=(const FcitxVinputAddon &) = delete;
  FcitxVinputAddon(FcitxVinputAddon &&) = delete;
  FcitxVinputAddon &operator=(FcitxVinputAddon &&) = delete;

  fcitx::Instance *instance() const {
    return instance_;
  }
  const FrontendBridge &bridge() const {
    return bridge_;
  }

private:
  fcitx::Instance *instance_ = nullptr;
  FrontendBridge bridge_;
};

} // namespace vinput_fcitx_bridge
