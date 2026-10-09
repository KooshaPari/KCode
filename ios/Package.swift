// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "KcodeKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
    ],
    products: [
        .library(name: "KcodeKit", targets: ["KcodeKit"])
    ],
    targets: [
        .target(
            name: "KcodeKit",
            swiftSettings: [.enableUpcomingFeature("StrictConcurrency")]
        ),
        .testTarget(
            name: "KcodeKitTests",
            dependencies: ["KcodeKit"]
        ),
    ]
)
