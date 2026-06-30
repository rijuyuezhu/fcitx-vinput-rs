#pragma once

#include "vinput_fcitx_bridge/fcitx_outcome.h"
#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <memory>
#include <string>
#include <string_view>
#include <vector>

#include <fcitx-utils/handlertable.h>
#include <fcitx/addoninstance.h>
#include <fcitx/event.h>
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
  AppliedOutcome TriggerNormal(fcitx::InputContext *ic,
                               std::string_view scene_id = "raw");

private:
  void HandleKeyEvent(fcitx::Event &event);

  fcitx::Instance *instance_ = nullptr;
  FrontendBridge bridge_;
  std::unique_ptr<SdBusDaemonClient> daemon_client_;
  std::vector<std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>>>
      event_handlers_;
};

} // namespace vinput_fcitx_bridge
