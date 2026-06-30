#include "vinput_fcitx_bridge/fcitx_outcome.h"

#include "vinput_fcitx_bridge/fcitx_candidates.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

void SetPreedit(fcitx::InputContext *ic, std::string_view text) {
  fcitx::Text preedit;
  preedit.append(std::string(text));
  ic->inputPanel().setPreedit(preedit);
  ic->updatePreedit();
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

void ClearPreedit(fcitx::InputContext *ic) {
  SetPreedit(ic, "");
}

std::string_view CommitText(const BridgeOutcome &outcome) {
  if (!outcome.text.empty()) {
    return outcome.text;
  }
  return outcome.payload.commit_text;
}

bool ShowCandidateMenu(fcitx::InputContext *ic, const RecognitionPayload &payload) {
  auto candidate_list = BuildResultCandidateList(payload);
  if (candidate_list == nullptr) {
    return false;
  }
  ClearPreedit(ic);
  ic->inputPanel().setCandidateList(std::move(candidate_list));
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  return true;
}

} // namespace

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *ic) {
  if (ic == nullptr) {
    return AppliedOutcome::None;
  }

  switch (outcome.kind) {
  case BridgeOutcome::Kind::None:
    return AppliedOutcome::None;
  case BridgeOutcome::Kind::Preedit:
  case BridgeOutcome::Kind::Error:
    SetPreedit(ic, outcome.text);
    return AppliedOutcome::Preedit;
  case BridgeOutcome::Kind::Commit: {
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    ClearPreedit(ic);
    ic->commitString(std::string(text));
    return AppliedOutcome::Commit;
  }
  case BridgeOutcome::Kind::CandidateMenu:
    if (ShowCandidateMenu(ic, outcome.payload)) {
      return AppliedOutcome::CandidateMenu;
    }
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    ClearPreedit(ic);
    ic->commitString(std::string(text));
    return AppliedOutcome::Commit;
  }

  return AppliedOutcome::None;
}

} // namespace vinput_fcitx_bridge
