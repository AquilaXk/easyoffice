// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "EasyOffice",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(
            name: "EasyOfficeDesignSystem",
            targets: ["EasyOfficeDesignSystem"]
        ),
        .library(
            name: "EasyOfficeCore",
            targets: ["EasyOfficeCore"]
        ),
        .executable(
            name: "EasyOfficeApp",
            targets: ["EasyOfficeApp"]
        )
    ],
    targets: [
        .target(
            name: "EasyOfficeDesignSystem",
            dependencies: []
        ),
        .target(
            name: "EasyOfficeCore",
            dependencies: ["EasyOfficeDesignSystem"]
        ),
        .executableTarget(
            name: "EasyOfficeApp",
            dependencies: [
                "EasyOfficeDesignSystem",
                "EasyOfficeCore"
            ]
        ),
        .testTarget(
            name: "EasyOfficeTests",
            dependencies: [
                "EasyOfficeDesignSystem",
                "EasyOfficeCore"
            ]
        )
    ]
)
