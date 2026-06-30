#pragma once

#include <string_view>

namespace vinput_fcitx_bridge::dbus {

inline constexpr std::string_view kFcitxBusName = "org.fcitx.Fcitx5";
inline constexpr std::string_view kServiceBusName = "org.fcitx.Vinput";
inline constexpr std::string_view kServiceObjectPath = "/org/fcitx/Vinput";
inline constexpr std::string_view kServiceInterface = "org.fcitx.Vinput.Service";
inline constexpr std::string_view kFrontendNotifierObjectPath =
    "/org/fcitx/Fcitx5/Vinput";
inline constexpr std::string_view kFrontendNotifierInterface =
    "org.fcitx.Fcitx5.Vinput1";

inline constexpr std::string_view kMethodStartRecording = "StartRecording";
inline constexpr std::string_view kMethodStartCommandRecording =
    "StartCommandRecording";
inline constexpr std::string_view kMethodStopRecording = "StopRecording";
inline constexpr std::string_view kMethodGetStatus = "GetStatus";
inline constexpr std::string_view kMethodGetAsrBackendState =
    "GetAsrBackendState";
inline constexpr std::string_view kMethodReloadAsrBackend = "ReloadAsrBackend";
inline constexpr std::string_view kMethodStartAdapter = "StartAdapter";
inline constexpr std::string_view kMethodStopAdapter = "StopAdapter";
inline constexpr std::string_view kMethodNotify = "Notify";

inline constexpr std::string_view kSignalRecognitionResult =
    "RecognitionResult";
inline constexpr std::string_view kSignalRecognitionPartial =
    "RecognitionPartial";
inline constexpr std::string_view kSignalStatusChanged = "StatusChanged";
inline constexpr std::string_view kSignalDaemonNotification =
    "DaemonNotification";

inline constexpr std::string_view kErrorOperationFailed =
    "org.fcitx.Vinput.Error.OperationFailed";

inline constexpr std::string_view kStatusIdle = "idle";
inline constexpr std::string_view kStatusRecording = "recording";
inline constexpr std::string_view kStatusInferring = "inferring";
inline constexpr std::string_view kStatusPostprocessing = "postprocessing";
inline constexpr std::string_view kStatusError = "error";

} // namespace vinput_fcitx_bridge::dbus
