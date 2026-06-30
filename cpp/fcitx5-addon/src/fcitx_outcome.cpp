#include "vinput_fcitx_bridge/fcitx_outcome.h"

#include "vinput_fcitx_bridge/fcitx_candidates.h"
#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

void SetPreedit(fcitx::InputContext *ic, std::string_view text) {
  ClearResultCandidateMenu(ic);
  fcitx::Text preedit;
  preedit.append(std::string(text));
  ic->inputPanel().setPreedit(preedit);
  ic->updatePreedit();
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

void ClearPreedit(fcitx::InputContext *ic) {
  SetPreedit(ic, "");
}

void DeleteSelectedTextIfAny(fcitx::InputContext *ic) {
  if (ic == nullptr) {
    return;
  }
  auto range = SelectedTextDeletionRange(ic->surroundingText());
  if (!range.has_value()) {
    return;
  }
  ic->deleteSurroundingText(range->offset, range->size);
}

std::string_view CommitText(const BridgeOutcome &outcome) {
  if (!outcome.text.empty()) {
    return outcome.text;
  }
  return outcome.payload.commit_text;
}

bool ShowCandidateMenu(fcitx::InputContext *ic, const RecognitionPayload &payload,
                       bool command_mode) {
  auto candidate_list = BuildResultCandidateList(
      payload,
      [command_mode](fcitx::InputContext *input_context, const Candidate &candidate) {
        ApplyResultCandidateSelection(input_context, candidate, command_mode);
      });
  if (candidate_list == nullptr) {
    return false;
  }
  ClearPreedit(ic);
  fcitx::Text aux_up;
  aux_up.append(ResultCandidateMenuTitle(payload.candidates.size()));
  ic->inputPanel().setAuxUp(aux_up);
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
  case BridgeOutcome::Kind::Clear:
    ClearPreedit(ic);
    return AppliedOutcome::Clear;
  case BridgeOutcome::Kind::Commit: {
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    if (outcome.command_mode) {
      DeleteSelectedTextIfAny(ic);
    }
    ClearResultCandidateMenu(ic);
    ClearPreedit(ic);
    ic->commitString(std::string(text));
    return AppliedOutcome::Commit;
  }
  case BridgeOutcome::Kind::CandidateMenu:
    if (ShowCandidateMenu(ic, outcome.payload, outcome.command_mode)) {
      return AppliedOutcome::CandidateMenu;
    }
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    if (outcome.command_mode) {
      DeleteSelectedTextIfAny(ic);
    }
    ClearResultCandidateMenu(ic);
    ClearPreedit(ic);
    ic->commitString(std::string(text));
    return AppliedOutcome::Commit;
  }

  return AppliedOutcome::None;
}

} // namespace vinput_fcitx_bridge
