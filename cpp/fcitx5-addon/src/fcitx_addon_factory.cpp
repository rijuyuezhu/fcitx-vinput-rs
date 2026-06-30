#include "vinput_fcitx_bridge/fcitx_addon.h"

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>

namespace vinput_fcitx_bridge {

class FcitxVinputAddonFactory final : public fcitx::AddonFactory {
public:
  fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
    return new FcitxVinputAddon(manager != nullptr ? manager->instance() : nullptr);
  }
};

} // namespace vinput_fcitx_bridge

#ifdef VINPUT_FCITX5_CORE_HAVE_ADDON_FACTORY_V2
FCITX_ADDON_FACTORY_V2(vinput, vinput_fcitx_bridge::FcitxVinputAddonFactory);
#else
FCITX_ADDON_FACTORY(vinput_fcitx_bridge::FcitxVinputAddonFactory);
#endif
