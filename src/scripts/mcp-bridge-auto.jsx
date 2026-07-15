// mcp-bridge-auto.jsx
// MCP Bridge command runtime for After Effects (headless Startup or ScriptUI fallback)
#targetengine "ae_mcp_bridge"

(function (thisObj) {

// Remove #include directives as we define functions below
/*
#include "createComposition.jsx"
#include "createTextLayer.jsx"
#include "createShapeLayer.jsx"
#include "createSolidLayer.jsx"
#include "setLayerProperties.jsx"
*/

// --- Function Definitions ---
var AE_MCP_BRIDGE_VERSION = "0.5.0";
var fxDialogsSuppressed = false;
var aeMcpBootstrapConfig = $.global.__adobeMcpBridgeBootstrapConfig || {};
var aeMcpHeadless = aeMcpBootstrapConfig.headless === true;
var aeMcpLifecycleMode = aeMcpHeadless ? "startup-headless" : "scriptui-panel";
var aeMcpPreviousInstanceId = "";
var aeMcpPreviousRuntime = null;
var aeMcpPreviousState = null;
var aeMcpAttachExistingRuntime = false;

try {
    aeMcpPreviousRuntime = $.global.__adobeMcpBridgeRuntime;
    if (aeMcpPreviousRuntime && aeMcpPreviousRuntime.getState) {
        aeMcpPreviousState = aeMcpPreviousRuntime.getState();
        aeMcpPreviousInstanceId = aeMcpPreviousState.instanceId || "";
        aeMcpAttachExistingRuntime = !aeMcpHeadless &&
            aeMcpPreviousState.running === true &&
            aeMcpPreviousState.lifecycleMode === "startup-headless";
    }
} catch (_previousRuntimeStateErr) {}

function attachDiagnosticsPanel(runtime) {
    var attachedPanel = (thisObj instanceof Panel)
        ? thisObj
        : new Window("palette", "MCP Bridge Auto " + AE_MCP_BRIDGE_VERSION, undefined, { resizeable: true });
    var attachedDockable = attachedPanel instanceof Panel;
    attachedPanel.orientation = "column";
    attachedPanel.alignChildren = ["fill", "top"];
    attachedPanel.spacing = 8;
    attachedPanel.margins = 12;

    var attachedStatus = attachedPanel.add("statictext", undefined, "", { multiline: true });
    attachedStatus.preferredSize.width = 280;
    var attachedRefresh = attachedPanel.add("button", undefined, "Refresh Headless Status");
    var attachedRestart = attachedPanel.add("button", undefined, "Restart Headless Bridge");

    function refreshAttachedStatus() {
        try {
            var runtimeState = runtime.getState();
            attachedStatus.text = "Headless runtime: " + (runtimeState.running ? "RUNNING" : "STOPPED") +
                "\nInstance: " + (runtimeState.instanceId || "") +
                "\nRuntime: " + (runtimeState.runtimeId || "");
        } catch (error) {
            attachedStatus.text = "Headless runtime unavailable: " + error.toString();
        }
        try { attachedPanel.layout.layout(true); } catch (_attachedLayoutErr) {}
    }

    attachedRefresh.onClick = refreshAttachedStatus;
    attachedRestart.onClick = function () {
        try {
            runtime.restart();
        } catch (_attachedRestartErr) {}
        refreshAttachedStatus();
    };
    attachedPanel.onClose = function () {
        // This window only observes the Startup-owned runtime. Closing it must
        // never stop the headless command and heartbeat schedules.
        return true;
    };
    attachedPanel.onResizing = attachedPanel.onResize = function () {
        this.layout.resize();
    };
    refreshAttachedStatus();
    attachedPanel.layout.layout(true);
    if (!attachedDockable) {
        attachedPanel.center();
        attachedPanel.show();
    }
}

if (aeMcpAttachExistingRuntime) {
    attachDiagnosticsPanel(aeMcpPreviousRuntime);
    return;
}

// A reload can otherwise leave repeating scheduleTask callbacks alive. Ask the
// previous runtime to cancel its task IDs before installing this generation.
try {
    if (aeMcpPreviousRuntime && aeMcpPreviousRuntime.stop) {
        aeMcpPreviousRuntime.stop({ removeHeartbeat: false, reason: "superseded" });
    }
} catch (_previousRuntimeStopErr) {}

// --- createComposition (from createComposition.jsx) --- 
function createComposition(args) {
    try {
        var name = args.name || "New Composition";
        var width = parseInt(args.width) || 1920;
        var height = parseInt(args.height) || 1080;
        var pixelAspect = parseFloat(args.pixelAspect) || 1.0;
        var duration = parseFloat(args.duration) || 10.0;
        var frameRate = parseFloat(args.frameRate) || 30.0;
        var bgColor = args.backgroundColor ? [args.backgroundColor.r/255, args.backgroundColor.g/255, args.backgroundColor.b/255] : [0, 0, 0];
        var newComp = app.project.items.addComp(name, width, height, pixelAspect, duration, frameRate);
        if (args.backgroundColor) {
            newComp.bgColor = bgColor;
        }
        return JSON.stringify({
            status: "success", message: "Composition created successfully",
            composition: { name: newComp.name, id: newComp.id, width: newComp.width, height: newComp.height, pixelAspect: newComp.pixelAspect, duration: newComp.duration, frameRate: newComp.frameRate, bgColor: newComp.bgColor }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({ status: "error", message: error.toString() }, null, 2);
    }
}

// --- createTextLayer (from createTextLayer.jsx) ---
function createTextLayer(args) {
    try {
        var compName = args.compName || "";
        var text = args.text || "Text Layer";
        var position = args.position || [960, 540]; 
        var fontSize = args.fontSize || 72;
        var color = args.color || [1, 1, 1]; 
        var startTime = args.startTime || 0;
        var duration = args.duration || 5; 
        var fontFamily = args.fontFamily || "Arial";
        var alignment = args.alignment || "center"; 
        var comp = null;
        for (var i = 1; i <= app.project.numItems; i++) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name === compName) { comp = item; break; }
        }
        if (!comp) {
            if (app.project.activeItem instanceof CompItem) { comp = app.project.activeItem; } 
            else { throw new Error("No composition found with name '" + compName + "' and no active composition"); }
        }
        var textLayer = comp.layers.addText(text);
        var textProp = textLayer.property("ADBE Text Properties").property("ADBE Text Document");
        var textDocument = textProp.value;
        textDocument.fontSize = fontSize;
        textDocument.fillColor = color;
        textDocument.font = fontFamily;
        if (alignment === "left") { textDocument.justification = ParagraphJustification.LEFT_JUSTIFY; } 
        else if (alignment === "center") { textDocument.justification = ParagraphJustification.CENTER_JUSTIFY; } 
        else if (alignment === "right") { textDocument.justification = ParagraphJustification.RIGHT_JUSTIFY; }
        textProp.setValue(textDocument);
        textLayer.property("Position").setValue(position);
        textLayer.startTime = startTime;
        if (duration > 0) { textLayer.outPoint = startTime + duration; }
        return JSON.stringify({
            status: "success", message: "Text layer created successfully",
            layer: { name: textLayer.name, index: textLayer.index, id: fxGetLayerId(textLayer), type: "text", inPoint: textLayer.inPoint, outPoint: textLayer.outPoint, position: textLayer.property("Position").value }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({ status: "error", message: error.toString() }, null, 2);
    }
}

// --- createShapeLayer (from createShapeLayer.jsx) --- 
function createShapeLayer(args) {
    try {
        var compName = args.compName || "";
        var shapeType = args.shapeType || "rectangle"; 
        var position = args.position || [960, 540]; 
        var size = args.size || [200, 200]; 
        var fillColor = args.fillColor || [1, 0, 0]; 
        var strokeColor = args.strokeColor || [0, 0, 0]; 
        var strokeWidth = args.strokeWidth || 0; 
        var startTime = args.startTime || 0;
        var duration = args.duration || 5; 
        var name = args.name || "Shape Layer";
        var points = args.points || 5; 
        var comp = null;
        for (var i = 1; i <= app.project.numItems; i++) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name === compName) { comp = item; break; }
        }
        if (!comp) {
            if (app.project.activeItem instanceof CompItem) { comp = app.project.activeItem; } 
            else { throw new Error("No composition found with name '" + compName + "' and no active composition"); }
        }
        var shapeLayer = comp.layers.addShape();
        shapeLayer.name = name;
        var contents = shapeLayer.property("Contents"); 
        var shapeGroup = contents.addProperty("ADBE Vector Group");
        var groupContents = shapeGroup.property("Contents"); 
        var shapePathProperty;
        if (shapeType === "rectangle") {
            shapePathProperty = groupContents.addProperty("ADBE Vector Shape - Rect");
            shapePathProperty.property("Size").setValue(size);
        } else if (shapeType === "ellipse") {
            shapePathProperty = groupContents.addProperty("ADBE Vector Shape - Ellipse");
            shapePathProperty.property("Size").setValue(size);
        } else if (shapeType === "polygon" || shapeType === "star") { 
            shapePathProperty = groupContents.addProperty("ADBE Vector Shape - Star");
            shapePathProperty.property("Type").setValue(shapeType === "polygon" ? 1 : 2); 
            shapePathProperty.property("Points").setValue(points);
            shapePathProperty.property("Outer Radius").setValue(size[0] / 2);
            if (shapeType === "star") { shapePathProperty.property("Inner Radius").setValue(size[0] / 3); }
        }
        var fill = groupContents.addProperty("ADBE Vector Graphic - Fill");
        fill.property("Color").setValue(fillColor);
        fill.property("Opacity").setValue(100);
        if (strokeWidth > 0) {
            var stroke = groupContents.addProperty("ADBE Vector Graphic - Stroke");
            stroke.property("Color").setValue(strokeColor);
            stroke.property("Stroke Width").setValue(strokeWidth);
            stroke.property("Opacity").setValue(100);
        }
        shapeLayer.property("Position").setValue(position);
        shapeLayer.startTime = startTime;
        if (duration > 0) { shapeLayer.outPoint = startTime + duration; }
        return JSON.stringify({
            status: "success", message: "Shape layer created successfully",
            layer: { name: shapeLayer.name, index: shapeLayer.index, id: fxGetLayerId(shapeLayer), type: "shape", shapeType: shapeType, inPoint: shapeLayer.inPoint, outPoint: shapeLayer.outPoint, position: shapeLayer.property("Position").value }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({ status: "error", message: error.toString() }, null, 2);
    }
}

// --- createSolidLayer (from createSolidLayer.jsx) --- 
function createSolidLayer(args) {
    try {
        var compName = args.compName || "";
        var color = args.color || [1, 1, 1]; 
        var name = args.name || "Solid Layer";
        var position = args.position || [960, 540]; 
        var size = args.size; 
        var startTime = args.startTime || 0;
        var duration = args.duration || 5; 
        var isAdjustment = args.isAdjustment || false; 
        var comp = null;
        for (var i = 1; i <= app.project.numItems; i++) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name === compName) { comp = item; break; }
        }
        if (!comp) {
            if (app.project.activeItem instanceof CompItem) { comp = app.project.activeItem; } 
            else { throw new Error("No composition found with name '" + compName + "' and no active composition"); }
        }
        if (!size) { size = [comp.width, comp.height]; }
        var solidLayer;
        if (isAdjustment) {
            solidLayer = comp.layers.addSolid([0, 0, 0], name, size[0], size[1], 1);
            solidLayer.adjustmentLayer = true;
        } else {
            solidLayer = comp.layers.addSolid(color, name, size[0], size[1], 1);
        }
        solidLayer.property("Position").setValue(position);
        solidLayer.startTime = startTime;
        if (duration > 0) { solidLayer.outPoint = startTime + duration; }
        return JSON.stringify({
            status: "success", message: isAdjustment ? "Adjustment layer created successfully" : "Solid layer created successfully",
            layer: { name: solidLayer.name, index: solidLayer.index, id: fxGetLayerId(solidLayer), type: isAdjustment ? "adjustment" : "solid", inPoint: solidLayer.inPoint, outPoint: solidLayer.outPoint, position: solidLayer.property("Position").value, isAdjustment: solidLayer.adjustmentLayer }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({ status: "error", message: error.toString() }, null, 2);
    }
}

// --- setLayerProperties (modified to handle text properties) ---
function setLayerProperties(args) {
    try {
        var compName = args.compName || "";
        var layerName = args.layerName || "";
        var layerIndex = args.layerIndex; 
        
        // General Properties
        var position = args.position; 
        var scale = args.scale; 
        var rotation = args.rotation; 
        var opacity = args.opacity; 
        var startTime = args.startTime; 
        var duration = args.duration; 

        // Text Specific Properties
        var textContent = args.text; // New: text content
        var fontFamily = args.fontFamily; // New: font family
        var fontSize = args.fontSize; // New: font size
        var fillColor = args.fillColor; // New: font color
        
        // Find the composition (same logic as before)
        var comp = null;
        for (var i = 1; i <= app.project.numItems; i++) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name === compName) { comp = item; break; }
        }
        if (!comp) {
            if (app.project.activeItem instanceof CompItem) { comp = app.project.activeItem; } 
            else { throw new Error("No composition found with name '" + compName + "' and no active composition"); }
        }
        
        // Find the layer (same logic as before)
        var layer = null;
        if (layerIndex !== undefined && layerIndex !== null) {
            if (layerIndex > 0 && layerIndex <= comp.numLayers) { layer = comp.layer(layerIndex); } 
            else { throw new Error("Layer index out of bounds: " + layerIndex); }
        } else if (layerName) {
            for (var j = 1; j <= comp.numLayers; j++) {
                if (comp.layer(j).name === layerName) { layer = comp.layer(j); break; }
            }
        }
        if (!layer) { throw new Error("Layer not found: " + (layerName || "index " + layerIndex)); }
        
        var changedProperties = [];
        var textDocumentChanged = false;
        var textProp = null;
        var textDocument = null;

        // --- Text Property Handling ---
        if (layer instanceof TextLayer && (textContent !== undefined || fontFamily !== undefined || fontSize !== undefined || fillColor !== undefined)) {
            var sourceTextProp = layer.property("Source Text");
            if (sourceTextProp && sourceTextProp.value) {
                var currentTextDocument = sourceTextProp.value; // Get the current value
                var updated = false;

                if (textContent !== undefined && textContent !== null && currentTextDocument.text !== textContent) {
                    currentTextDocument.text = textContent;
                    changedProperties.push("text");
                    updated = true;
                }
                if (fontFamily !== undefined && fontFamily !== null && currentTextDocument.font !== fontFamily) {
                    // Add basic validation/logging for font existence if needed
                    // try { app.fonts.findFont(fontFamily); } catch (e) { logToPanel("Warning: Font '"+fontFamily+"' might not be installed."); }
                    currentTextDocument.font = fontFamily;
                    changedProperties.push("fontFamily");
                    updated = true;
                }
                if (fontSize !== undefined && fontSize !== null && currentTextDocument.fontSize !== fontSize) {
                    currentTextDocument.fontSize = fontSize;
                    changedProperties.push("fontSize");
                    updated = true;
                }
                // Comparing colors needs care due to potential floating point inaccuracies if set via UI
                // Simple comparison for now
                if (fillColor !== undefined && fillColor !== null && 
                    (currentTextDocument.fillColor[0] !== fillColor[0] || 
                     currentTextDocument.fillColor[1] !== fillColor[1] || 
                     currentTextDocument.fillColor[2] !== fillColor[2])) {
                    currentTextDocument.fillColor = fillColor;
                    changedProperties.push("fillColor");
                    updated = true;
                }

                // Only set the value if something actually changed
                if (updated) {
                    try {
                        sourceTextProp.setValue(currentTextDocument);
                        logToPanel("Applied changes to Text Document for layer: " + layer.name);
                    } catch (e) {
                        logToPanel("ERROR applying Text Document changes: " + e.toString());
                        // Decide if we should throw or just log the error for text properties
                        // For now, just log, other properties might still succeed
                    }
                }
                 // Store the potentially updated document for the return value
                 textDocument = currentTextDocument; 

            } else {
                logToPanel("Warning: Could not access Source Text property for layer: " + layer.name);
            }
        }

        // --- General Property Handling ---
        if (position !== undefined && position !== null) { layer.property("Position").setValue(position); changedProperties.push("position"); }
        if (scale !== undefined && scale !== null) { layer.property("Scale").setValue(scale); changedProperties.push("scale"); }
        if (rotation !== undefined && rotation !== null) {
            if (layer.threeDLayer) { 
                // For 3D layers, Z rotation is often what's intended by a single value
                layer.property("Z Rotation").setValue(rotation);
            } else { 
                layer.property("Rotation").setValue(rotation); 
            }
            changedProperties.push("rotation");
        }
        if (opacity !== undefined && opacity !== null) { layer.property("Opacity").setValue(opacity); changedProperties.push("opacity"); }
        if (startTime !== undefined && startTime !== null) { layer.startTime = startTime; changedProperties.push("startTime"); }
        if (duration !== undefined && duration !== null && duration > 0) {
            var actualStartTime = (startTime !== undefined && startTime !== null) ? startTime : layer.startTime;
            layer.outPoint = actualStartTime + duration;
            changedProperties.push("duration");
        }

        // Return success with updated layer details (including text if changed)
        var returnLayerInfo = {
            name: layer.name,
            index: layer.index,
            position: layer.property("Position").value,
            scale: layer.property("Scale").value,
            rotation: layer.threeDLayer ? layer.property("Z Rotation").value : layer.property("Rotation").value, // Return appropriate rotation
            opacity: layer.property("Opacity").value,
            inPoint: layer.inPoint,
            outPoint: layer.outPoint,
            changedProperties: changedProperties
        };
        // Add text properties to the return object if it was a text layer
        if (layer instanceof TextLayer && textDocument) {
            returnLayerInfo.text = textDocument.text;
            returnLayerInfo.fontFamily = textDocument.font;
            returnLayerInfo.fontSize = textDocument.fontSize;
            returnLayerInfo.fillColor = textDocument.fillColor;
        }

        // *** ADDED LOGGING HERE ***
        logToPanel("Final check before return:");
        logToPanel("  Changed Properties: " + changedProperties.join(", "));
        logToPanel("  Return Layer Info Font: " + (returnLayerInfo.fontFamily || "N/A")); 
        logToPanel("  TextDocument Font: " + (textDocument ? textDocument.font : "N/A"));

        return JSON.stringify({
            status: "success", message: "Layer properties updated successfully",
            layer: returnLayerInfo
        }, null, 2);
    } catch (error) {
        // Error handling remains similar, but add more specific checks if needed
        return JSON.stringify({ status: "error", message: error.toString() }, null, 2);
    }
}

/**
 * Sets a keyframe for a specific property on a layer.
 * Indices are 1-based for After Effects collections.
 * @param {number} compIndex - The index of the composition (1-based).
 * @param {number} layerIndex - The index of the layer within the composition (1-based).
 * @param {string} propertyName - The name of the property (e.g., "Position", "Scale", "Rotation", "Opacity").
 * @param {number} timeInSeconds - The time (in seconds) for the keyframe.
 * @param {any} value - The value for the keyframe (e.g., [x, y] for Position, [w, h] for Scale, angle for Rotation, percentage for Opacity).
 * @returns {string} JSON string indicating success or error.
 */
function setLayerKeyframe(compIndex, layerIndex, propertyName, timeInSeconds, value) {
    try {
        // Use 1-based indices as per After Effects API
        var comp = app.project.items[compIndex];
        if (!comp || !(comp instanceof CompItem)) {
            return JSON.stringify({ success: false, message: "Composition not found at index " + compIndex });
        }
        var layer = comp.layers[layerIndex];
        if (!layer) {
            return JSON.stringify({ success: false, message: "Layer not found at index " + layerIndex + " in composition '" + comp.name + "'"});
        }

        var transformGroup = layer.property("Transform");
        if (!transformGroup) {
             return JSON.stringify({ success: false, message: "Transform properties not found for layer '" + layer.name + "' (type: " + layer.matchName + ")." });
        }

        var property = transformGroup.property(propertyName);
        if (!property) {
            // Check other common property groups if not in Transform
             if (layer.property("Effects") && layer.property("Effects").property(propertyName)) {
                 property = layer.property("Effects").property(propertyName);
             } else if (layer.property("Text") && layer.property("Text").property(propertyName)) {
                 property = layer.property("Text").property(propertyName);
            } // Add more groups if needed (e.g., Masks, Shapes)

            if (!property) {
                 return JSON.stringify({ success: false, message: "Property '" + propertyName + "' not found on layer '" + layer.name + "'." });
            }
        }


        // Ensure the property can be keyframed
        if (!property.canVaryOverTime) {
             return JSON.stringify({ success: false, message: "Property '" + propertyName + "' cannot be keyframed." });
        }

        // Make sure the property is enabled for keyframing
        if (property.numKeys === 0 && !property.isTimeVarying) {
             property.setValueAtTime(comp.time, property.value); // Set initial keyframe if none exist
        }


        property.setValueAtTime(timeInSeconds, value);

        return JSON.stringify({ success: true, message: "Keyframe set for '" + propertyName + "' on layer '" + layer.name + "' at " + timeInSeconds + "s." });
    } catch (e) {
        return JSON.stringify({ success: false, message: "Error setting keyframe: " + e.toString() + " (Line: " + e.line + ")" });
    }
}


/**
 * Sets an expression for a specific property on a layer.
 * @param {number} compIndex - The index of the composition (1-based).
 * @param {number} layerIndex - The index of the layer within the composition (1-based).
 * @param {string} propertyName - The name of the property (e.g., "Position", "Scale", "Rotation", "Opacity").
 * @param {string} expressionString - The JavaScript expression string. Use "" to remove expression.
 * @returns {string} JSON string indicating success or error.
 */
function setLayerExpression(compIndex, layerIndex, propertyName, expressionString) {
    try {
         // Adjust indices to be 0-based for ExtendScript arrays
        var comp = app.project.items[compIndex];
         if (!comp || !(comp instanceof CompItem)) {
            return JSON.stringify({ success: false, message: "Composition not found at index " + compIndex });
        }
        var layer = comp.layers[layerIndex];
         if (!layer) {
            return JSON.stringify({ success: false, message: "Layer not found at index " + layerIndex + " in composition '" + comp.name + "'"});
        }

        var transformGroup = layer.property("Transform");
         if (!transformGroup) {
             // Allow expressions on non-transformable layers if property exists elsewhere
             // return JSON.stringify({ success: false, message: "Transform properties not found for layer '" + layer.name + "' (type: " + layer.matchName + ")." });
        }

        var property = transformGroup ? transformGroup.property(propertyName) : null;
         if (!property) {
            // Check other common property groups if not in Transform
             if (layer.property("Effects") && layer.property("Effects").property(propertyName)) {
                 property = layer.property("Effects").property(propertyName);
             } else if (layer.property("Text") && layer.property("Text").property(propertyName)) {
                 property = layer.property("Text").property(propertyName);
             } // Add more groups if needed

            if (!property) {
                 return JSON.stringify({ success: false, message: "Property '" + propertyName + "' not found on layer '" + layer.name + "'." });
            }
        }

        if (!property.canSetExpression) {
            return JSON.stringify({ success: false, message: "Property '" + propertyName + "' does not support expressions." });
        }

        property.expression = expressionString;

        var action = expressionString === "" ? "removed" : "set";
        return JSON.stringify({ success: true, message: "Expression " + action + " for '" + propertyName + "' on layer '" + layer.name + "'." });
    } catch (e) {
        return JSON.stringify({ success: false, message: "Error setting expression: " + e.toString() + " (Line: " + e.line + ")" });
    }
}

// --- applyEffect/applyEffectTemplate helpers ---
function fxListOwnKeys(obj) {
    var keys = [];
    if (!obj) {
        return keys;
    }
    for (var k in obj) {
        if (obj.hasOwnProperty(k)) {
            keys.push(k);
        }
    }
    return keys;
}

function fxHasOwnEntries(obj) {
    if (!obj) {
        return false;
    }
    for (var k in obj) {
        if (obj.hasOwnProperty(k)) {
            return true;
        }
    }
    return false;
}

function fxGetBooleanArg(args, key, defaultValue) {
    if (!args || args[key] === undefined || args[key] === null) {
        return defaultValue === true;
    }
    return args[key] === true;
}

function fxGetNumberArg(value, fallback) {
    if (value === undefined || value === null || value === "") {
        return fallback;
    }
    var parsed = parseFloat(value);
    if (isNaN(parsed)) {
        return fallback;
    }
    return parsed;
}

function fxEnsureParentFolder(file) {
    if (!file || !file.parent) {
        return;
    }
    if (!file.parent.exists) {
        file.parent.create();
    }
}

function fxDateToIsoString(dateObj) {
    var d = dateObj || new Date();
    if (d && typeof d.toISOString === "function") {
        return d.toISOString();
    }
    try {
        return d.toUTCString();
    } catch (_e) {
        return "" + d;
    }
}

function fxSetSuppressDialogs(enabled) {
    if (enabled) {
        if (!fxDialogsSuppressed) {
            app.beginSuppressDialogs();
            fxDialogsSuppressed = true;
        }
    } else {
        if (fxDialogsSuppressed) {
            app.endSuppressDialogs();
            fxDialogsSuppressed = false;
        }
    }
}

function fxWithSuppressDialogs(enabled, fn) {
    var shouldSet = (enabled === true || enabled === false);
    var previous = fxDialogsSuppressed;
    if (shouldSet) {
        fxSetSuppressDialogs(enabled === true);
    }
    var result;
    try {
        result = fn();
    } finally {
        if (shouldSet) {
            fxSetSuppressDialogs(previous);
        }
    }
    return result;
}

function fxParseNumericId(value) {
    if (value === undefined || value === null || value === "") {
        return null;
    }
    var parsed = parseInt(value, 10);
    if (isNaN(parsed)) {
        return null;
    }
    return parsed;
}

function fxGetCompId(comp) {
    try {
        if (comp && comp.id !== undefined && comp.id !== null) {
            var parsed = parseInt(comp.id, 10);
            if (!isNaN(parsed)) {
                return parsed;
            }
        }
    } catch (_e) {}
    return null;
}

function fxGetLayerId(layer) {
    try {
        if (layer && layer.id !== undefined && layer.id !== null) {
            var parsed = parseInt(layer.id, 10);
            if (!isNaN(parsed)) {
                return parsed;
            }
        }
    } catch (_e) {}
    return null;
}

function fxResolveComposition(args) {
    var compId = fxParseNumericId(args.compId);
    var compName = args.compName || args.compositionName;
    var compIndex = args.compIndex;
    var i;

    if (compId !== null) {
        for (i = 1; i <= app.project.numItems; i++) {
            var byId = app.project.item(i);
            if (byId instanceof CompItem && fxGetCompId(byId) === compId) {
                return byId;
            }
        }
        throw new Error("Composition not found by id " + compId);
    }

    if (compName) {
        for (i = 1; i <= app.project.numItems; i++) {
            var byName = app.project.item(i);
            if (byName instanceof CompItem && byName.name === compName) {
                return byName;
            }
        }
        throw new Error("Composition not found by name '" + compName + "'");
    }

    if (compIndex !== undefined && compIndex !== null) {
        var parsedCompIndex = parseInt(compIndex, 10);
        if (!isNaN(parsedCompIndex)) {
            var byIndex = app.project.item(parsedCompIndex);
            if (byIndex && byIndex instanceof CompItem) {
                return byIndex;
            }
            throw new Error("Composition not found at index " + parsedCompIndex);
        }
    }

    if (app.project.activeItem && app.project.activeItem instanceof CompItem) {
        return app.project.activeItem;
    }

    for (i = 1; i <= app.project.numItems; i++) {
        var firstComp = app.project.item(i);
        if (firstComp instanceof CompItem) {
            return firstComp;
        }
    }

    throw new Error("No composition found in project");
}

function fxResolveLayer(comp, args) {
    var layerId = fxParseNumericId(args.layerId);
    var layerName = args.layerName;
    var layerIndex = args.layerIndex;
    var j;

    if (layerId !== null) {
        for (j = 1; j <= comp.numLayers; j++) {
            var byId = comp.layer(j);
            if (fxGetLayerId(byId) === layerId) {
                return byId;
            }
        }
        throw new Error("Layer not found by id " + layerId + " in composition '" + comp.name + "'");
    }

    if (layerName) {
        for (j = 1; j <= comp.numLayers; j++) {
            if (comp.layer(j).name === layerName) {
                return comp.layer(j);
            }
        }
        throw new Error("Layer not found by name '" + layerName + "' in composition '" + comp.name + "'");
    }

    if (layerIndex !== undefined && layerIndex !== null) {
        var parsedLayerIndex = parseInt(layerIndex, 10);
        if (!isNaN(parsedLayerIndex) && parsedLayerIndex > 0 && parsedLayerIndex <= comp.numLayers) {
            return comp.layer(parsedLayerIndex);
        }
        throw new Error("Layer index out of bounds: " + layerIndex + " in composition '" + comp.name + "'");
    }

    if (comp.selectedLayers && comp.selectedLayers.length > 0) {
        return comp.selectedLayers[0];
    }

    if (comp.numLayers > 0) {
        return comp.layer(1);
    }

    throw new Error("No layer found in composition '" + comp.name + "'");
}

function fxPushUnique(targetArray, value) {
    if (!value || typeof value !== "string") {
        return;
    }
    for (var i = 0; i < targetArray.length; i++) {
        if (targetArray[i] === value) {
            return;
        }
    }
    targetArray.push(value);
}

function fxAddEffectWithFallback(layer, effectName, effectMatchName) {
    var candidates = [];
    var failures = [];

    fxPushUnique(candidates, effectMatchName);
    fxPushUnique(candidates, effectName);

    var isRampRequest =
        effectMatchName === "ADBE Ramp" ||
        effectName === "ADBE Ramp" ||
        effectName === "Ramp" ||
        effectName === "Gradient Ramp";

    if (isRampRequest) {
        fxPushUnique(candidates, "ADBE Ramp");
        fxPushUnique(candidates, "Ramp");
        fxPushUnique(candidates, "ADBE 4ColorGradient");
        fxPushUnique(candidates, "ADBE 4 Color Gradient");
    }

    if (candidates.length === 0) {
        throw new Error("No valid effect identifier was provided");
    }

    for (var i = 0; i < candidates.length; i++) {
        var candidate = candidates[i];
        try {
            var effect = layer.Effects.addProperty(candidate);
            if (effect) {
                return {
                    effect: effect,
                    identifier: candidate
                };
            }
        } catch (e) {
            failures.push(candidate + ": " + e.toString());
        }
    }

    throw new Error(
        "Failed to add effect. Tried identifiers: " +
        candidates.join(", ") +
        (failures.length ? " | details: " + failures.join(" ; ") : "")
    );
}

// --- applyEffect (from applyEffect.jsx) ---
function applyEffect(args) {
    try {
        var effectName = args.effectName;
        var effectMatchName = args.effectMatchName;
        var presetPath = args.presetPath;
        var effectSettings = args.effectSettings || {};

        if (!effectName && !effectMatchName && !presetPath) {
            throw new Error("You must specify either effectName, effectMatchName, or presetPath");
        }

        var comp = fxResolveComposition(args);
        var layer = fxResolveLayer(comp, args);
        var effectResult;

        if (presetPath) {
            var presetFile = new File(presetPath);
            if (!presetFile.exists) {
                throw new Error("Effect preset file not found: " + presetPath);
            }

            layer.applyPreset(presetFile);
            effectResult = {
                type: "preset",
                name: presetPath.split("/").pop().split("\\").pop(),
                applied: true
            };
        } else {
            var added = fxAddEffectWithFallback(layer, effectName, effectMatchName);
            effectResult = {
                type: "effect",
                name: added.effect.name,
                matchName: added.effect.matchName,
                index: added.effect.propertyIndex,
                usedIdentifier: added.identifier
            };
            applyEffectSettings(added.effect, effectSettings);
        }

        return JSON.stringify({
            status: "success",
            message: "Effect applied successfully",
            effect: effectResult,
            layer: {
                name: layer.name,
                index: layer.index,
                id: fxGetLayerId(layer)
            },
            composition: {
                name: comp.name,
                id: comp.id
            }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

// Helper function to apply effect settings
function applyEffectSettings(effect, settings) {
    if (!fxHasOwnEntries(settings)) {
        return;
    }

    var settingKeys = fxListOwnKeys(settings);
    for (var x = 0; x < settingKeys.length; x++) {
        var propName = settingKeys[x];
        try {
            var property = null;
            try {
                property = effect.property(propName);
            } catch (e) {
                for (var i = 1; i <= effect.numProperties; i++) {
                    var prop = effect.property(i);
                    if (prop.name === propName) {
                        property = prop;
                        break;
                    }
                }
            }
            if (property && property.setValue) {
                property.setValue(settings[propName]);
            }
        } catch (e2) {
            $.writeln("Error setting effect property '" + propName + "': " + e2.toString());
        }
    }
}

function fxApplyTemplateSettings(effect, settings) {
    if (!settings) {
        return;
    }
    var keys = fxListOwnKeys(settings);
    for (var i = 0; i < keys.length; i++) {
        var propName = keys[i];
        try {
            var property = effect.property(propName);
            if (property) {
                property.setValue(settings[propName]);
            }
        } catch (e) {
            $.writeln("Warning: Could not set " + propName + " on effect " + effect.name + ": " + e);
        }
    }
}

function fxAddTemplateEffectWithFallback(layer, primaryMatchName, fallbackMatchNames) {
    var candidates = [];
    var failures = [];
    fxPushUnique(candidates, primaryMatchName);

    if (fallbackMatchNames && fallbackMatchNames.length) {
        for (var i = 0; i < fallbackMatchNames.length; i++) {
            fxPushUnique(candidates, fallbackMatchNames[i]);
        }
    }

    for (var x = 0; x < candidates.length; x++) {
        var candidate = candidates[x];
        try {
            var effect = layer.Effects.addProperty(candidate);
            if (effect) {
                return {
                    effect: effect,
                    identifier: candidate
                };
            }
        } catch (e) {
            failures.push(candidate + ": " + e.toString());
        }
    }

    throw new Error(
        "Failed to add template effect. Tried identifiers: " +
        candidates.join(", ") +
        (failures.length ? " | details: " + failures.join(" ; ") : "")
    );
}

// --- applyEffectTemplate (from applyEffectTemplate.jsx) ---
function applyEffectTemplate(args) {
    try {
        var templateName = args.templateName;
        var customSettings = args.customSettings || {};

        if (!templateName) {
            throw new Error("You must specify a templateName");
        }

        var comp = fxResolveComposition(args);
        var layer = fxResolveLayer(comp, args);

        var templates = {
            "gaussian-blur": {
                effects: [
                    {
                        effectMatchName: "ADBE Gaussian Blur 2",
                        settings: { "Blurriness": customSettings.blurriness || 20 }
                    }
                ]
            },
            "directional-blur": {
                effects: [
                    {
                        effectMatchName: "ADBE Directional Blur",
                        settings: {
                            "Direction": customSettings.direction || 0,
                            "Blur Length": customSettings.length || 10
                        }
                    }
                ]
            },
            "color-balance": {
                effects: [
                    {
                        effectMatchName: "ADBE Color Balance (HLS)",
                        settings: {
                            "Hue": customSettings.hue || 0,
                            "Lightness": customSettings.lightness || 0,
                            "Saturation": customSettings.saturation || 0
                        }
                    }
                ]
            },
            "brightness-contrast": {
                effects: [
                    {
                        effectMatchName: "ADBE Brightness & Contrast 2",
                        settings: {
                            "Brightness": customSettings.brightness || 0,
                            "Contrast": customSettings.contrast || 0,
                            "Use Legacy": false
                        }
                    }
                ]
            },
            "curves": {
                effects: [
                    {
                        effectMatchName: "ADBE CurvesCustom",
                        settings: {}
                    }
                ]
            },
            "glow": {
                effects: [
                    {
                        effectMatchName: "ADBE Glow",
                        settings: {
                            "Glow Threshold": customSettings.threshold || 50,
                            "Glow Radius": customSettings.radius || 15,
                            "Glow Intensity": customSettings.intensity || 1
                        }
                    }
                ]
            },
            "drop-shadow": {
                effects: [
                    {
                        effectMatchName: "ADBE Drop Shadow",
                        settings: {
                            "Shadow Color": customSettings.color || [0, 0, 0, 1],
                            "Opacity": customSettings.opacity || 50,
                            "Direction": customSettings.direction || 135,
                            "Distance": customSettings.distance || 10,
                            "Softness": customSettings.softness || 10
                        }
                    }
                ]
            },
            "smooth-gradient": {
                effects: [
                    {
                        effectMatchName: "ADBE Ramp",
                        fallbackMatchNames: ["Ramp", "ADBE 4ColorGradient", "ADBE 4 Color Gradient"],
                        settings: {
                            "Start Color": customSettings.startColor || [0.02, 0.08, 0.2],
                            "End Color": customSettings.endColor || [0.75, 0.85, 1.0],
                            "Start of Ramp": customSettings.startPoint || [0, 0],
                            "End of Ramp": customSettings.endPoint || [1920, 1080],
                            "Ramp Scatter": customSettings.scatter || 0,
                            "Blend With Original": customSettings.blend || 0
                        }
                    }
                ]
            },
            "cinematic-look": {
                effects: [
                    {
                        effectMatchName: "ADBE CurvesCustom",
                        settings: {}
                    },
                    {
                        effectMatchName: "ADBE Vibrance",
                        settings: {
                            "Vibrance": 15,
                            "Saturation": -5
                        }
                    }
                ]
            },
            "text-pop": {
                effects: [
                    {
                        effectMatchName: "ADBE Drop Shadow",
                        settings: {
                            "Shadow Color": [0, 0, 0, 1],
                            "Opacity": 75,
                            "Distance": 5,
                            "Softness": 10
                        }
                    },
                    {
                        effectMatchName: "ADBE Glow",
                        settings: {
                            "Glow Threshold": 50,
                            "Glow Radius": 10,
                            "Glow Intensity": 1.5
                        }
                    }
                ]
            }
        };

        var template = templates[templateName];
        if (!template) {
            var availableTemplates = fxListOwnKeys(templates).join(", ");
            throw new Error("Template '" + templateName + "' not found. Available templates: " + availableTemplates);
        }

        var appliedEffects = [];
        var effects = template.effects || [];
        for (var i = 0; i < effects.length; i++) {
            var effectData = effects[i];
            var added = fxAddTemplateEffectWithFallback(layer, effectData.effectMatchName, effectData.fallbackMatchNames || []);
            fxApplyTemplateSettings(added.effect, effectData.settings || {});
            appliedEffects.push({
                name: added.effect.name,
                matchName: added.effect.matchName,
                usedIdentifier: added.identifier
            });
        }

        return JSON.stringify({
            status: "success",
            message: "Effect template '" + templateName + "' applied successfully",
            appliedEffects: appliedEffects,
            layer: {
                name: layer.name,
                index: layer.index,
                id: fxGetLayerId(layer)
            },
            composition: {
                name: comp.name,
                id: comp.id
            }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function fxHasExplicitLayerTarget(args) {
    return args.layerId !== undefined ||
        args.layerName !== undefined ||
        args.layerIndex !== undefined;
}

function fxResolveProbeLayer(comp, args) {
    if (fxHasExplicitLayerTarget(args)) {
        return {
            layer: fxResolveLayer(comp, args),
            cleanup: false
        };
    }

    if (comp.selectedLayers && comp.selectedLayers.length > 0) {
        return {
            layer: comp.selectedLayers[0],
            cleanup: false
        };
    }

    if (comp.numLayers > 0) {
        return {
            layer: comp.layer(1),
            cleanup: false
        };
    }

    var probeLayer = comp.layers.addSolid(
        [0.5, 0.5, 0.5],
        "__mcp_probe_layer__",
        Math.max(1, comp.width),
        Math.max(1, comp.height),
        1,
        Math.max(1, comp.duration)
    );
    probeLayer.enabled = false;
    return {
        layer: probeLayer,
        cleanup: true
    };
}

function fxKnownEffectsCatalog() {
    return [
        { name: "Gaussian Blur", matchName: "ADBE Gaussian Blur 2", category: "Blur & Sharpen" },
        { name: "Directional Blur", matchName: "ADBE Directional Blur", category: "Blur & Sharpen" },
        { name: "Brightness & Contrast", matchName: "ADBE Brightness & Contrast 2", category: "Color Correction" },
        { name: "Color Balance (HLS)", matchName: "ADBE Color Balance (HLS)", category: "Color Correction" },
        { name: "Curves", matchName: "ADBE CurvesCustom", category: "Color Correction" },
        { name: "Vibrance", matchName: "ADBE Vibrance", category: "Color Correction" },
        { name: "Glow", matchName: "ADBE Glow", category: "Stylize" },
        { name: "Drop Shadow", matchName: "ADBE Drop Shadow", category: "Perspective" },
        { name: "Gradient Ramp", matchName: "ADBE Ramp", category: "Generate" },
        { name: "4 Color Gradient", matchName: "ADBE 4ColorGradient", category: "Generate" },
        { name: "Fractal Noise", matchName: "ADBE Fractal Noise", category: "Noise & Grain" },
        { name: "Noise", matchName: "ADBE Noise", category: "Noise & Grain" }
    ];
}

function fxProbeEffect(layer, entry) {
    var candidates = [];
    var failures = [];
    fxPushUnique(candidates, entry.matchName);
    fxPushUnique(candidates, entry.name);

    if (entry.matchName === "ADBE Ramp") {
        fxPushUnique(candidates, "Ramp");
        fxPushUnique(candidates, "ADBE 4ColorGradient");
        fxPushUnique(candidates, "ADBE 4 Color Gradient");
    }

    for (var i = 0; i < candidates.length; i++) {
        var candidate = candidates[i];
        var addedEffect = null;
        try {
            addedEffect = layer.Effects.addProperty(candidate);
            if (addedEffect) {
                var resolvedName = addedEffect.name;
                var resolvedMatchName = addedEffect.matchName;
                try {
                    addedEffect.remove();
                } catch (_removeErr) {}
                return {
                    available: true,
                    usedIdentifier: candidate,
                    resolvedName: resolvedName,
                    resolvedMatchName: resolvedMatchName
                };
            }
        } catch (e) {
            failures.push(candidate + ": " + e.toString());
        }
    }

    return {
        available: false,
        error: failures.join(" ; ")
    };
}

function fxSerializeValue(value) {
    if (value === null || value === undefined) {
        return value;
    }
    var t = typeof value;
    if (t === "number" || t === "string" || t === "boolean") {
        return value;
    }
    if (value instanceof Array) {
        var arr = [];
        for (var i = 0; i < value.length; i++) {
            arr.push(fxSerializeValue(value[i]));
        }
        return arr;
    }
    try {
        return value.toString();
    } catch (_e) {}
    return null;
}

function fxDescribeEffectProperties(effect) {
    var properties = [];

    for (var i = 1; i <= effect.numProperties; i++) {
        var prop = effect.property(i);
        if (!prop) {
            continue;
        }

        var row = {
            index: i,
            name: prop.name || "",
            matchName: prop.matchName || "",
            propertyValueType: prop.propertyValueType,
            canSetExpression: !!prop.canSetExpression
        };

        try {
            if (prop.hasMin) {
                row.minValue = prop.minValue;
            }
        } catch (_minErr) {}

        try {
            if (prop.hasMax) {
                row.maxValue = prop.maxValue;
            }
        } catch (_maxErr) {}

        try {
            row.currentValue = fxSerializeValue(prop.value);
        } catch (_valueErr) {}

        properties.push(row);
    }

    return properties;
}

function listSupportedEffects(args) {
    var probeContext = null;

    try {
        var options = args || {};
        var includeUnavailable = options.includeUnavailable === true;
        var comp = fxResolveComposition(options);
        probeContext = fxResolveProbeLayer(comp, options);
        var probeLayer = probeContext.layer;
        var catalog = fxKnownEffectsCatalog();
        var effects = [];
        var availableCount = 0;
        var unavailableCount = 0;

        for (var i = 0; i < catalog.length; i++) {
            var entry = catalog[i];
            var probe = fxProbeEffect(probeLayer, entry);
            if (probe.available) {
                availableCount++;
            } else {
                unavailableCount++;
            }
            if (probe.available || includeUnavailable) {
                effects.push({
                    name: entry.name,
                    matchName: entry.matchName,
                    category: entry.category,
                    available: probe.available,
                    usedIdentifier: probe.usedIdentifier || null,
                    resolvedName: probe.resolvedName || null,
                    resolvedMatchName: probe.resolvedMatchName || null,
                    error: probe.error || null
                });
            }
        }

        return JSON.stringify({
            status: "success",
            composition: {
                id: fxGetCompId(comp),
                name: comp.name
            },
            probeLayer: {
                id: fxGetLayerId(probeLayer),
                name: probeLayer.name,
                index: probeLayer.index,
                temporary: probeContext.cleanup
            },
            summary: {
                catalogSize: catalog.length,
                availableCount: availableCount,
                unavailableCount: unavailableCount
            },
            effects: effects
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    } finally {
        if (probeContext && probeContext.cleanup && probeContext.layer) {
            try {
                probeContext.layer.remove();
            } catch (_cleanupError) {}
        }
    }
}

function describeEffect(args) {
    var probeContext = null;
    var added = null;

    try {
        var options = args || {};
        var effectName = options.effectName;
        var effectMatchName = options.effectMatchName;
        if (!effectName && !effectMatchName) {
            throw new Error("You must specify either effectName or effectMatchName");
        }

        var comp = fxResolveComposition(options);
        probeContext = fxResolveProbeLayer(comp, options);
        var layer = probeContext.layer;
        added = fxAddEffectWithFallback(layer, effectName, effectMatchName);
        var effect = added.effect;
        var properties = fxDescribeEffectProperties(effect);

        return JSON.stringify({
            status: "success",
            composition: {
                id: fxGetCompId(comp),
                name: comp.name
            },
            layer: {
                id: fxGetLayerId(layer),
                name: layer.name,
                index: layer.index,
                temporary: probeContext.cleanup
            },
            effect: {
                name: effect.name,
                matchName: effect.matchName,
                usedIdentifier: added.identifier,
                propertyCount: properties.length,
                properties: properties
            }
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    } finally {
        if (added && added.effect) {
            try {
                added.effect.remove();
            } catch (_removeEffectErr) {}
        }
        if (probeContext && probeContext.cleanup && probeContext.layer) {
            try {
                probeContext.layer.remove();
            } catch (_cleanupErr) {}
        }
    }
}

function fxResolveRenderQueueItem(queueIndex) {
    var rq = app.project.renderQueue;
    if (!rq) {
        throw new Error("Render queue is not available");
    }
    var index = parseInt(queueIndex, 10);
    if (isNaN(index) || index < 1) {
        throw new Error("queueIndex must be a positive integer");
    }
    var item = null;
    try {
        if (rq.item) {
            item = rq.item(index);
        }
    } catch (_e) {}
    if (!item && rq.items) {
        try {
            item = rq.items[index];
        } catch (_e2) {}
        if (!item && index > 0) {
            try {
                item = rq.items[index - 1];
            } catch (_e3) {}
        }
    }
    if (!item) {
        throw new Error("Render queue item not found at index " + index);
    }
    return item;
}

function fxGetRenderStatusName(statusValue) {
    if (statusValue === null || statusValue === undefined) {
        return null;
    }
    if (typeof RQItemStatus === "undefined") {
        return "" + statusValue;
    }
    try {
        if (statusValue === RQItemStatus.UNQUEUED) return "UNQUEUED";
        if (statusValue === RQItemStatus.QUEUED) return "QUEUED";
        if (statusValue === RQItemStatus.NEEDS_OUTPUT) return "NEEDS_OUTPUT";
        if (statusValue === RQItemStatus.RENDERING) return "RENDERING";
        if (statusValue === RQItemStatus.USER_STOPPED) return "USER_STOPPED";
        if (statusValue === RQItemStatus.ERR_STOPPED) return "ERR_STOPPED";
        if (statusValue === RQItemStatus.DONE) return "DONE";
        if (statusValue === RQItemStatus.WILL_CONTINUE) return "WILL_CONTINUE";
    } catch (_e) {}
    return "" + statusValue;
}

function fxRenderQueueSnapshot(includeItems) {
    var rq = app.project ? app.project.renderQueue : null;
    if (!rq) {
        return {
            available: false,
            isRendering: false,
            totalItems: 0,
            items: []
        };
    }

    var total = 0;
    try {
        total = rq.numItems || 0;
    } catch (_e) {}

    var snapshot = {
        available: true,
        isRendering: false,
        totalItems: total,
        items: []
    };

    try {
        snapshot.isRendering = rq.rendering === true;
    } catch (_e2) {}

    if (includeItems === false) {
        return snapshot;
    }

    for (var i = 1; i <= total; i++) {
        var item = null;
        try {
            item = rq.item(i);
        } catch (_itemErr) {}
        if (!item) {
            continue;
        }

        var statusValue = null;
        try {
            statusValue = item.status;
        } catch (_statusErr) {}

        var outputPath = null;
        try {
            var om = item.outputModule(1);
            if (om && om.file) {
                outputPath = om.file.fsName;
            }
        } catch (_omErr) {}

        var compInfo = null;
        try {
            if (item.comp) {
                compInfo = {
                    id: fxGetCompId(item.comp),
                    name: item.comp.name
                };
            }
        } catch (_compErr) {}

        snapshot.items.push({
            queueIndex: i,
            status: statusValue,
            statusName: fxGetRenderStatusName(statusValue),
            render: item.render === true,
            outputPath: outputPath,
            composition: compInfo
        });
    }

    return snapshot;
}

function fxRenderItemDiagnostics(item, queueIndex) {
    var statusValue = null;
    try {
        statusValue = item.status;
    } catch (_statusErr) {}

    var outputPath = null;
    var outputExists = null;
    try {
        var om = item.outputModule(1);
        if (om && om.file) {
            outputPath = om.file.fsName;
            outputExists = om.file.exists === true;
        }
    } catch (_omErr) {}

    var startTime = null;
    try {
        if (item.startTime) {
            startTime = item.startTime.toString();
        }
    } catch (_startErr) {}

    var endTime = null;
    try {
        if (item.endTime) {
            endTime = item.endTime.toString();
        }
    } catch (_endErr) {}

    return {
        queueIndex: queueIndex,
        status: statusValue,
        statusName: fxGetRenderStatusName(statusValue),
        render: item.render === true,
        outputPath: outputPath,
        outputExists: outputExists,
        startTime: startTime,
        endTime: endTime
    };
}

function fxCanToggleRenderFlag(statusValue) {
    if (typeof RQItemStatus === "undefined") {
        return true;
    }
    try {
        if (statusValue === RQItemStatus.QUEUED) return true;
        if (statusValue === RQItemStatus.UNQUEUED) return true;
        if (statusValue === RQItemStatus.NEEDS_OUTPUT) return true;
        return false;
    } catch (_e) {
        return true;
    }
}

function fxResolveCloseOption(rawValue, defaultName) {
    if (typeof CloseOptions === "undefined") {
        throw new Error("CloseOptions is not available in this host.");
    }
    var name = rawValue;
    if (name === undefined || name === null || name === "") {
        name = defaultName || "DO_NOT_SAVE_CHANGES";
    }
    name = ("" + name).toUpperCase();

    if (name === "SAVE" || name === "SAVE_CHANGES") {
        return {
            key: "SAVE_CHANGES",
            value: CloseOptions.SAVE_CHANGES
        };
    }
    if (
        name === "DO_NOT_SAVE" ||
        name === "DONT_SAVE" ||
        name === "DO_NOT_SAVE_CHANGES"
    ) {
        return {
            key: "DO_NOT_SAVE_CHANGES",
            value: CloseOptions.DO_NOT_SAVE_CHANGES
        };
    }
    if (name === "PROMPT" || name === "PROMPT_TO_SAVE_CHANGES") {
        return {
            key: "PROMPT_TO_SAVE_CHANGES",
            value: CloseOptions.PROMPT_TO_SAVE_CHANGES
        };
    }
    throw new Error("Unsupported closeOption: " + rawValue);
}

function fxCurrentProjectInfo() {
    var proj = app.project;
    var path = null;
    if (proj && proj.file) {
        path = proj.file.fsName;
    }
    var dirty = null;
    try {
        dirty = proj.dirty;
    } catch (_e) {}
    return {
        name: (proj && proj.file) ? proj.file.name : "Untitled Project",
        path: path,
        dirty: dirty,
        numItems: proj ? proj.numItems : 0
    };
}

function fxCloseProjectWithMode(options, defaultCloseOptionName, interactive) {
    if (!app.project) {
        throw new Error("No project is currently open.");
    }
    var closeOption = fxResolveCloseOption(options.closeOption, defaultCloseOptionName || "DO_NOT_SAVE_CHANGES");
    var saveAsPath = options.saveAsPath || options.filePath || options.path || null;

    if (closeOption.key === "PROMPT_TO_SAVE_CHANGES") {
        if (!interactive) {
            throw new Error(
                "PROMPT_TO_SAVE_CHANGES is not supported in non-interactive mode. " +
                "Use interactive=true with suppressDialogs=false, or use SAVE_CHANGES/DO_NOT_SAVE_CHANGES."
            );
        }
    }

    if (closeOption.key === "SAVE_CHANGES") {
        var existingFile = null;
        try {
            existingFile = app.project.file;
        } catch (_fileErr) {}

        if (existingFile && existingFile.exists === true) {
            app.project.save();
        } else {
            if (saveAsPath) {
                var saveFile = new File(saveAsPath);
                fxEnsureParentFolder(saveFile);
                app.project.save(saveFile);
            } else if (!interactive) {
                throw new Error(
                    "Current project has no concrete file path. " +
                    "Provide saveAsPath/filePath/path before closing with SAVE_CHANGES in non-interactive mode."
                );
            }
            // interactive mode without explicit path is allowed:
            // app.project.close(SAVE_CHANGES) may show Save As dialog for user input.
        }
    }

    app.project.close(closeOption.value);
    return closeOption.key;
}

function saveFramePng(args) {
    try {
        var options = args || {};
        var outputPath = options.outputPath;
        if (!outputPath) {
            throw new Error("outputPath is required");
        }
        var comp = fxResolveComposition(options);
        var timeSeconds = fxGetNumberArg(options.timeSeconds, comp.time);
        var overwrite = fxGetBooleanArg(options, "overwrite", true);
        var suppressDialogs = fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            var file = new File(outputPath);
            fxEnsureParentFolder(file);
            if (file.exists) {
                if (!overwrite) {
                    throw new Error("output file already exists");
                }
                if (!file.remove()) {
                    throw new Error("failed to remove existing file");
                }
            }
            comp.saveFrameToPng(timeSeconds, file);
            return JSON.stringify({
                status: "success",
                composition: {
                    id: fxGetCompId(comp),
                    name: comp.name
                },
                timeSeconds: timeSeconds,
                outputPath: file.fsName,
                fileExists: file.exists === true
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function renderQueueAdd(args) {
    try {
        var options = args || {};
        var outputPath = options.outputPath;
        if (!outputPath) {
            throw new Error("outputPath is required");
        }
        var comp = fxResolveComposition(options);
        var suppressDialogs = fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            var rqItem = app.project.renderQueue.items.add(comp);
            var renderSettingsTemplate = options.renderSettingsTemplate;
            if (renderSettingsTemplate && renderSettingsTemplate !== "") {
                rqItem.applyTemplate(renderSettingsTemplate);
            }
            var timeSpanStart = fxGetNumberArg(options.timeSpanStart, null);
            if (timeSpanStart !== null) {
                rqItem.timeSpanStart = timeSpanStart;
            }
            var timeSpanDuration = fxGetNumberArg(options.timeSpanDuration, null);
            if (timeSpanDuration !== null) {
                rqItem.timeSpanDuration = timeSpanDuration;
            }
            var outputModule = rqItem.outputModule(1);
            var outputModuleTemplate = options.outputModuleTemplate;
            if (outputModuleTemplate && outputModuleTemplate !== "") {
                outputModule.applyTemplate(outputModuleTemplate);
            }
            var file = new File(outputPath);
            fxEnsureParentFolder(file);
            outputModule.file = file;

            var queueIndex = (rqItem.index !== undefined && rqItem.index !== null)
                ? rqItem.index
                : app.project.renderQueue.numItems;

            return JSON.stringify({
                status: "success",
                composition: {
                    id: fxGetCompId(comp),
                    name: comp.name
                },
                queueIndex: queueIndex,
                outputPath: file.fsName,
                renderSettingsTemplate: renderSettingsTemplate || null,
                outputModuleTemplate: outputModuleTemplate || null
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function renderQueueStatus(args) {
    try {
        var options = args || {};
        var queueIndex = options.queueIndex;
        if (queueIndex === undefined || queueIndex === null) {
            throw new Error("queueIndex is required");
        }
        var item = fxResolveRenderQueueItem(queueIndex);
        var outputPath = null;
        try {
            var om = item.outputModule(1);
            if (om && om.file) {
                outputPath = om.file.fsName;
            }
        } catch (_omErr) {}
        var compInfo = null;
        try {
            if (item.comp) {
                compInfo = {
                    id: fxGetCompId(item.comp),
                    name: item.comp.name
                };
            }
        } catch (_compErr) {}
        var statusValue = null;
        try {
            statusValue = item.status;
        } catch (_statusErr) {}
        var elapsedSeconds = null;
        try {
            elapsedSeconds = item.elapsedSeconds;
        } catch (_elapsedErr) {}
        var startTime = null;
        try {
            if (item.startTime) {
                startTime = item.startTime.toString();
            }
        } catch (_startErr) {}
        var endTime = null;
        try {
            if (item.endTime) {
                endTime = item.endTime.toString();
            }
        } catch (_endErr) {}
        var renderEnabled = null;
        try {
            renderEnabled = item.render;
        } catch (_renderErr) {}

        return JSON.stringify({
            status: "success",
            queueIndex: parseInt(queueIndex, 10),
            renderStatus: statusValue,
            elapsedSeconds: elapsedSeconds,
            startTime: startTime,
            endTime: endTime,
            render: renderEnabled,
            composition: compInfo,
            outputPath: outputPath
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function renderQueueStart(args) {
    var renderStateBackup = null;
    var previousOnError = null;
    var renderErrors = [];
    try {
        var options = args || {};
        var suppressDialogs = fxGetBooleanArg(options, "suppressDialogs", true);
        var includeItems = fxGetBooleanArg(options, "includeItems", true);
        var targetQueueIndex = fxGetNumberArg(options.queueIndex, null);
        if (targetQueueIndex !== null) {
            targetQueueIndex = parseInt(targetQueueIndex, 10);
            if (isNaN(targetQueueIndex) || targetQueueIndex < 1) {
                throw new Error("queueIndex must be a positive integer");
            }
        }

        return fxWithSuppressDialogs(suppressDialogs, function () {
            var rq = app.project.renderQueue;
            if (!rq) {
                throw new Error("Render queue is not available");
            }
            if (!rq.numItems || rq.numItems < 1) {
                throw new Error("Render queue is empty");
            }

            if (targetQueueIndex !== null) {
                fxResolveRenderQueueItem(targetQueueIndex);
                renderStateBackup = [];
                for (var i = 1; i <= rq.numItems; i++) {
                    var item = fxResolveRenderQueueItem(i);
                    var currentRender = item.render === true;
                    var desiredRender = (i === targetQueueIndex);
                    if (currentRender === desiredRender) {
                        continue;
                    }

                    var statusValue = null;
                    try {
                        statusValue = item.status;
                    } catch (_statusErr) {}

                    if (!fxCanToggleRenderFlag(statusValue)) {
                        if (i === targetQueueIndex) {
                            throw new Error(
                                "Cannot enable rendering for queue item " + i +
                                " with status " + fxGetRenderStatusName(statusValue) + "."
                            );
                        }
                        continue;
                    }

                    item.render = desiredRender;
                    renderStateBackup.push({
                        queueIndex: i,
                        render: currentRender
                    });
                }
            }

            var targetBefore = null;
            if (targetQueueIndex !== null) {
                targetBefore = fxRenderItemDiagnostics(
                    fxResolveRenderQueueItem(targetQueueIndex),
                    targetQueueIndex
                );
            }

            previousOnError = app.onError;
            app.onError = function (errString, severityString) {
                try {
                    renderErrors.push({
                        message: "" + errString,
                        severity: severityString ? ("" + severityString) : null,
                        timestamp: fxDateToIsoString(new Date())
                    });
                } catch (_captureErr) {}
            };

            var before = fxRenderQueueSnapshot(includeItems);
            var startedAt = new Date();
            rq.render();
            var finishedAt = new Date();
            var after = fxRenderQueueSnapshot(includeItems);

            var targetAfter = null;
            var targetDone = null;
            if (targetQueueIndex !== null) {
                var targetItem = fxResolveRenderQueueItem(targetQueueIndex);
                targetAfter = fxRenderItemDiagnostics(targetItem, targetQueueIndex);
                try {
                    targetDone = targetItem.status === RQItemStatus.DONE;
                } catch (_doneErr) {
                    targetDone = null;
                }
            }

            var outputExists = null;
            if (targetAfter) {
                outputExists = targetAfter.outputExists;
            }

            var blockingErrors = [];
            for (var eIdx = 0; eIdx < renderErrors.length; eIdx++) {
                var evt = renderErrors[eIdx];
                var sev = evt && evt.severity ? ("" + evt.severity).toUpperCase() : "";
                if (sev === "PROGRESS" || sev === "INFO") {
                    continue;
                }
                blockingErrors.push(evt);
            }

            var successState = true;
            if (blockingErrors.length > 0) {
                successState = false;
            }
            if (targetDone === false) {
                successState = false;
            }
            if (outputExists === false) {
                successState = false;
            }

            return JSON.stringify({
                status: successState ? "success" : "error",
                message: successState
                    ? "Render queue processing completed."
                    : "Render queue did not finish cleanly. See diagnostics.",
                startedAt: fxDateToIsoString(startedAt),
                finishedAt: fxDateToIsoString(finishedAt),
                durationSeconds: (finishedAt.getTime() - startedAt.getTime()) / 1000,
                queueIndex: targetQueueIndex,
                targetBefore: targetBefore,
                targetAfter: targetAfter,
                targetDone: targetDone,
                outputExists: outputExists,
                errors: blockingErrors,
                events: renderErrors,
                before: before,
                after: after
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    } finally {
        try {
            app.onError = previousOnError;
        } catch (_restoreOnErrorErr) {}

        if (renderStateBackup && app.project && app.project.renderQueue) {
            for (var b = 0; b < renderStateBackup.length; b++) {
                try {
                    var state = renderStateBackup[b];
                    var rqItem = fxResolveRenderQueueItem(state.queueIndex);
                    rqItem.render = state.render;
                } catch (_restoreErr) {}
            }
        }
    }
}

function renderQueueIsRendering(args) {
    try {
        var options = args || {};
        var includeItems = fxGetBooleanArg(options, "includeItems", true);
        var snapshot = fxRenderQueueSnapshot(includeItems);
        var queueIndex = fxGetNumberArg(options.queueIndex, null);
        var targetItem = null;

        if (queueIndex !== null) {
            var item = fxResolveRenderQueueItem(queueIndex);
            var statusValue = null;
            try {
                statusValue = item.status;
            } catch (_statusErr) {}
            targetItem = {
                queueIndex: parseInt(queueIndex, 10),
                status: statusValue,
                statusName: fxGetRenderStatusName(statusValue),
                render: item.render === true
            };
        }

        return JSON.stringify({
            status: "success",
            isRendering: snapshot.isRendering === true,
            queue: snapshot,
            targetItem: targetItem
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function projectOpen(args) {
    try {
        var options = args || {};
        var filePath = options.filePath || options.path || options.projectPath;
        if (!filePath) {
            throw new Error("filePath is required");
        }
        var closeCurrent = fxGetBooleanArg(options, "closeCurrent", true);
        var closeOption = fxResolveCloseOption(options.closeOption, "DO_NOT_SAVE_CHANGES");
        var interactive = fxGetBooleanArg(options, "interactive", false);
        var suppressDialogs = interactive ? false : fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            if (closeCurrent && app.project) {
                closeOption.key = fxCloseProjectWithMode(options, "DO_NOT_SAVE_CHANGES", interactive);
            }
            var file = new File(filePath);
            if (!file.exists) {
                throw new Error("Project file not found: " + filePath);
            }
            app.open(file);
            return JSON.stringify({
                status: "success",
                action: "opened",
                project: fxCurrentProjectInfo(),
                closeOption: closeOption.key
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function projectClose(args) {
    try {
        var options = args || {};
        var closeOption = fxResolveCloseOption(options.closeOption, "DO_NOT_SAVE_CHANGES");
        var interactive = fxGetBooleanArg(options, "interactive", false);
        var suppressDialogs = interactive ? false : fxGetBooleanArg(options, "suppressDialogs", true);
        var before = fxCurrentProjectInfo();

        return fxWithSuppressDialogs(suppressDialogs, function () {
            closeOption.key = fxCloseProjectWithMode(options, "DO_NOT_SAVE_CHANGES", interactive);
            return JSON.stringify({
                status: "success",
                action: "closed",
                closeOption: closeOption.key,
                closedProject: before,
                currentProject: fxCurrentProjectInfo()
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function projectSave(args) {
    try {
        var options = args || {};
        var saveAsPath = options.saveAsPath || options.filePath || options.path || null;
        var interactive = fxGetBooleanArg(options, "interactive", false);
        var suppressDialogs = interactive ? false : fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            if (!app.project) {
                throw new Error("No project is currently open.");
            }

            if (saveAsPath) {
                var file = new File(saveAsPath);
                fxEnsureParentFolder(file);
                app.project.save(file);
            } else {
                var existingFile = null;
                try {
                    existingFile = app.project.file;
                } catch (_fileErr) {}

                if (!existingFile || existingFile.exists !== true) {
                    if (!interactive) {
                        throw new Error(
                            "Current project is not saved to a concrete file path. " +
                            "Provide saveAsPath/filePath/path to save without dialogs, or set interactive=true."
                        );
                    }
                }
                app.project.save();
            }

            return JSON.stringify({
                status: "success",
                action: "saved",
                project: fxCurrentProjectInfo()
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function projectSaveAs(args) {
    try {
        var options = args || {};
        var filePath = options.filePath || options.path || options.saveAsPath;
        if (!filePath) {
            throw new Error("filePath is required");
        }
        options.saveAsPath = filePath;
        return projectSave(options);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function applicationQuit(args) {
    try {
        var options = args || {};
        var closeProject = fxGetBooleanArg(options, "closeProject", true);
        var closeOption = fxResolveCloseOption(options.closeOption, "DO_NOT_SAVE_CHANGES");
        var interactive = fxGetBooleanArg(options, "interactive", false);
        var suppressDialogs = interactive ? false : fxGetBooleanArg(options, "suppressDialogs", true);
        var before = fxCurrentProjectInfo();

        return fxWithSuppressDialogs(suppressDialogs, function () {
            if (closeProject && app.project) {
                closeOption.key = fxCloseProjectWithMode(options, "DO_NOT_SAVE_CHANGES", interactive);
            } else if (!closeProject && app.project) {
                var dirty = false;
                try {
                    dirty = app.project.dirty === true;
                } catch (_dirtyErr) {}
                if (dirty && !interactive) {
                    throw new Error(
                        "closeProject=false is not allowed when the current project has unsaved changes. " +
                        "Use closeProject=true with closeOption/saveAsPath, or set interactive=true."
                    );
                }
            }
            return JSON.stringify({
                status: "success",
                action: "quit-requested",
                closeProject: closeProject,
                closeOption: closeOption.key,
                previousProject: before,
                currentProject: fxCurrentProjectInfo(),
                _quitRequested: true
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function setCurrentTime(args) {
    try {
        var options = args || {};
        var timeSeconds = fxGetNumberArg(options.timeSeconds, null);
        if (timeSeconds === null) {
            throw new Error("timeSeconds is required");
        }
        var comp = fxResolveComposition(options);
        var suppressDialogs = fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            comp.time = timeSeconds;
            return JSON.stringify({
                status: "success",
                composition: {
                    id: fxGetCompId(comp),
                    name: comp.name
                },
                timeSeconds: comp.time
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function getCurrentTime(args) {
    try {
        var options = args || {};
        var comp = fxResolveComposition(options);
        return JSON.stringify({
            status: "success",
            composition: {
                id: fxGetCompId(comp),
                name: comp.name
            },
            timeSeconds: comp.time
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function setWorkArea(args) {
    try {
        var options = args || {};
        var workAreaStart = fxGetNumberArg(options.workAreaStart, null);
        var workAreaDuration = fxGetNumberArg(options.workAreaDuration, null);
        if (workAreaStart === null || workAreaDuration === null) {
            throw new Error("workAreaStart and workAreaDuration are required");
        }
        var comp = fxResolveComposition(options);
        var suppressDialogs = fxGetBooleanArg(options, "suppressDialogs", true);

        return fxWithSuppressDialogs(suppressDialogs, function () {
            comp.workAreaStart = workAreaStart;
            comp.workAreaDuration = workAreaDuration;
            return JSON.stringify({
                status: "success",
                composition: {
                    id: fxGetCompId(comp),
                    name: comp.name
                },
                workAreaStart: comp.workAreaStart,
                workAreaDuration: comp.workAreaDuration
            }, null, 2);
        });
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function getWorkArea(args) {
    try {
        var options = args || {};
        var comp = fxResolveComposition(options);
        return JSON.stringify({
            status: "success",
            composition: {
                id: fxGetCompId(comp),
                name: comp.name
            },
            workAreaStart: comp.workAreaStart,
            workAreaDuration: comp.workAreaDuration
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function getCompositionMarkers(args) {
    try {
        var options = args || {};
        var comp = fxResolveComposition(options);
        var markerProperty = comp.markerProperty;
        var markers = [];

        if (markerProperty) {
            for (var i = 1; i <= markerProperty.numKeys; i++) {
                var markerValue = markerProperty.keyValue(i);
                var timeSeconds = markerProperty.keyTime(i);
                var marker = {
                    index: i,
                    timeSeconds: timeSeconds,
                    frame: Math.round(timeSeconds * comp.frameRate),
                    comment: markerValue.comment,
                    chapter: markerValue.chapter,
                    url: markerValue.url,
                    frameTarget: markerValue.frameTarget,
                    cuePointName: markerValue.cuePointName,
                    duration: markerValue.duration,
                    eventCuePoint: markerValue.eventCuePoint
                };

                try {
                    marker.protectedRegion = markerValue.protectedRegion;
                } catch (_protectedRegionError) {}

                try {
                    marker.label = markerValue.label;
                } catch (_labelError) {}

                try {
                    var rawParameters = markerValue.getParameters();
                    var parameters = {};
                    if (rawParameters) {
                        for (var p = 0; p < rawParameters.length; p += 2) {
                            parameters[rawParameters[p]] = rawParameters[p + 1];
                        }
                    }
                    marker.parameters = parameters;
                } catch (_parametersError) {
                    marker.parameters = {};
                }

                markers.push(marker);
            }
        }

        return JSON.stringify({
            status: "success",
            composition: {
                id: fxGetCompId(comp),
                name: comp.name,
                frameRate: comp.frameRate,
                duration: comp.duration
            },
            markerCount: markers.length,
            markers: markers
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function cleanupPreviewFolder(args) {
    try {
        var options = args || {};
        var folderPath = options.folderPath;
        if (!folderPath) {
            throw new Error("folderPath is required");
        }
        var extension = options.extension || "png";
        extension = extension.toLowerCase();
        if (extension.charAt(0) === ".") {
            extension = extension.substring(1);
        }
        var prefix = options.prefix;
        var maxAgeSeconds = fxGetNumberArg(options.maxAgeSeconds, null);

        var folder = new Folder(folderPath);
        if (!folder.exists) {
            return JSON.stringify({
                status: "success",
                folderPath: folder.fsName || folderPath,
                removedCount: 0,
                removed: [],
                message: "Folder does not exist"
            }, null, 2);
        }

        var now = new Date().getTime();
        var removed = [];
        var files = folder.getFiles();
        for (var i = 0; i < files.length; i++) {
            var file = files[i];
            if (!(file instanceof File)) {
                continue;
            }
            var lowerName = file.name.toLowerCase();
            if (extension) {
                var dotIndex = lowerName.lastIndexOf(".");
                var ext = (dotIndex >= 0) ? lowerName.substring(dotIndex + 1) : "";
                if (ext !== extension) {
                    continue;
                }
            }
            if (prefix && file.name.indexOf(prefix) !== 0) {
                continue;
            }
            if (maxAgeSeconds !== null && file.modified) {
                var ageSeconds = (now - file.modified.getTime()) / 1000;
                if (ageSeconds < maxAgeSeconds) {
                    continue;
                }
            }
            if (file.remove()) {
                removed.push(file.name);
            }
        }

        return JSON.stringify({
            status: "success",
            folderPath: folder.fsName,
            removedCount: removed.length,
            removed: removed
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function setSuppressDialogs(args) {
    try {
        var enabled = args && args.enabled === true;
        fxSetSuppressDialogs(enabled);
        return JSON.stringify({
            status: "success",
            enabled: fxDialogsSuppressed
        }, null, 2);
    } catch (error) {
        return JSON.stringify({
            status: "error",
            message: error.toString()
        }, null, 2);
    }
}

function getSuppressDialogs() {
    return JSON.stringify({
        status: "success",
        enabled: fxDialogsSuppressed
    }, null, 2);
}

// --- End of Function Definitions ---

// --- Bridge test function to verify communication and effects application ---
function bridgeTestEffects(args) {
    try {
        var compIndex = (args && args.compIndex) ? args.compIndex : 1;
        var layerIndex = (args && args.layerIndex) ? args.layerIndex : 1;

        // Apply a light Gaussian Blur
        var blurRes = JSON.parse(applyEffect({
            compIndex: compIndex,
            layerIndex: layerIndex,
            effectMatchName: "ADBE Gaussian Blur 2",
            effectSettings: { "Blurriness": 5 }
        }));

        // Apply a simple drop shadow via template
        var shadowRes = JSON.parse(applyEffectTemplate({
            compIndex: compIndex,
            layerIndex: layerIndex,
            templateName: "drop-shadow"
        }));

        return JSON.stringify({
            status: "success",
            message: "Bridge test effects applied.",
            results: [blurRes, shadowRes]
        }, null, 2);
    } catch (e) {
        return JSON.stringify({ status: "error", message: e.toString() }, null, 2);
    }
}

// JSON polyfill for ExtendScript (when JSON is undefined)
if (typeof JSON === "undefined") {
    JSON = {};
}
if (typeof JSON.parse !== "function") {
    JSON.parse = function (text) {
        // Safe-ish fallback for trusted input (our own command file)
        return eval("(" + text + ")");
    };
}
if (typeof JSON.stringify !== "function") {
    (function () {
        function esc(str) {
            return (str + "")
                .replace(/\\/g, "\\\\")
                .replace(/"/g, '\\"')
                .replace(/\n/g, "\\n")
                .replace(/\r/g, "\\r")
                .replace(/\t/g, "\\t");
        }
        function toJSON(val) {
            if (val === null) return "null";
            var t = typeof val;
            if (t === "number" || t === "boolean") return String(val);
            if (t === "string") return '"' + esc(val) + '"';
            if (val instanceof Array) {
                var a = [];
                for (var i = 0; i < val.length; i++) a.push(toJSON(val[i]));
                return "[" + a.join(",") + "]";
            }
            if (t === "object") {
                var props = [];
                for (var k in val) {
                    if (val.hasOwnProperty(k) && typeof val[k] !== "function" && typeof val[k] !== "undefined") {
                        props.push('"' + esc(k) + '":' + toJSON(val[k]));
                    }
                }
                return "{" + props.join(",") + "}";
            }
            return "null";
        }
        JSON.stringify = function (value, _replacer, _space) {
            return toJSON(value);
        };
    })();
}

// Startup mode deliberately creates no Window/Panel. ScriptUI remains a
// manual diagnostics fallback when this file is launched without bootstrap.
var panel = null;
var isDockablePanel = false;
var statusText = null;
var logPanel = null;
var logText = null;
var autoRunCheckbox = null;
var debugLogCheckbox = null;
var checkButton = null;

if (!aeMcpHeadless) {
    panel = (thisObj instanceof Panel)
        ? thisObj
        : new Window("palette", "MCP Bridge Auto " + AE_MCP_BRIDGE_VERSION, undefined, { resizeable: true });
    isDockablePanel = panel instanceof Panel;
    if (isDockablePanel) {
        panel.text = "MCP Bridge Auto " + AE_MCP_BRIDGE_VERSION;
    }
    panel.orientation = "column";
    panel.alignChildren = ["fill", "top"];
    panel.spacing = 10;
    panel.margins = 16;

    statusText = panel.add("statictext", undefined, "Waiting for commands... (v" + AE_MCP_BRIDGE_VERSION + ")");
    statusText.alignment = ["fill", "top"];

    logPanel = panel.add("panel", undefined, "Command Log");
    logPanel.orientation = "column";
    logPanel.alignChildren = ["fill", "fill"];
    logText = logPanel.add("edittext", undefined, "", {multiline: true, readonly: true});
    logText.preferredSize.height = 200;

    autoRunCheckbox = panel.add("checkbox", undefined, "Auto-run commands");
    autoRunCheckbox.value = true;

    debugLogCheckbox = panel.add("checkbox", undefined, "Write debug log");
    debugLogCheckbox.value = false;
}

// Check interval (ms)
var checkInterval = 2000;
var heartbeatInterval = 3000;
var isChecking = false;
var permissionStateKnown = false;
var hasFileNetworkPermission = false;
var hasShownPermissionDialog = false;
var autoRunValueBeforePermissionLock = true;
var commandCheckerTaskId = 0;
var heartbeatTaskId = 0;
var lastCommandCheckerState = "";
var debugLogEnabled = false;
var debugLogPathOverride = "";
var bridgeInstanceId = aeMcpPreviousInstanceId;
var currentRequestId = "";
var currentCommandName = "";
var bridgeRuntimeId = "runtime-" + (new Date().getTime()) + "-" + Math.floor(Math.random() * 1000000);
var bridgeStartedAt = fxDateToIsoString(new Date());
var bridgeRuntimeRunning = false;
var bridgeHasStarted = false;
var bridgeRuntime = null;

function isControlValid(control) {
    try {
        if (!control) return false;
        var _visible = control.visible;
        return true;
    } catch (_e) {
        return false;
    }
}

function setStatus(message) {
    try {
        if (isControlValid(statusText)) {
            statusText.text = message;
        }
    } catch (_e) {}
}

function isAutoRunEnabled() {
    try {
        if (isControlValid(autoRunCheckbox)) {
            return autoRunCheckbox.value === true;
        }
    } catch (_e) {}
    return true;
}

function safePanelUpdate() {
    try {
        if (panel) {
            panel.update();
        }
    } catch (_e) {}
}

function logCommandCheckerState(key, message) {
    if (lastCommandCheckerState === key) {
        return;
    }
    lastCommandCheckerState = key;
    logToPanel(message);
}

function getBridgeFolderPath() {
    return Folder.myDocuments.fsName + "/ae-mcp-bridge";
}

function ensureBridgeFolder() {
    var bridgeFolder = new Folder(getBridgeFolderPath());
    if (!bridgeFolder.exists) {
        if (!bridgeFolder.create()) {
            throw new Error("Unable to create bridge folder: " + bridgeFolder.fsName);
        }
    }
    return bridgeFolder;
}

function sanitizeBridgePathSegment(value) {
    return String(value || "unknown").replace(/[^A-Za-z0-9_.-]/g, "_");
}

function createBridgeInstanceId() {
    var version = "unknown";
    try {
        version = app.version || "unknown";
    } catch (_versionErr) {}
    return "ae-" +
        sanitizeBridgePathSegment(version) +
        "-" +
        (new Date().getTime()) +
        "-" +
        Math.floor(Math.random() * 1000000);
}

function getBridgeInstanceId() {
    if (!bridgeInstanceId) {
        bridgeInstanceId = createBridgeInstanceId();
    }
    return bridgeInstanceId;
}

function ensureInstanceFolder() {
    var root = ensureBridgeFolder();
    var instancesFolder = new Folder(root.fsName + "/instances");
    if (!instancesFolder.exists) {
        if (!instancesFolder.create()) {
            throw new Error("Unable to create instances folder: " + instancesFolder.fsName);
        }
    }
    var instanceFolder = new Folder(instancesFolder.fsName + "/" + getBridgeInstanceId());
    if (!instanceFolder.exists) {
        if (!instanceFolder.create()) {
            throw new Error("Unable to create instance folder: " + instanceFolder.fsName);
        }
    }
    return instanceFolder;
}

// Command file path - use Documents folder for reliable access
function getCommandFilePath() {
    var instanceFolder = ensureInstanceFolder();
    return instanceFolder.fsName + "/ae_command.json";
}

// Result file path - use Documents folder for reliable access
function getResultFilePath() {
    var instanceFolder = ensureInstanceFolder();
    return instanceFolder.fsName + "/ae_mcp_result.json";
}

function getHeartbeatFilePath() {
    var instanceFolder = ensureInstanceFolder();
    return instanceFolder.fsName + "/heartbeat.json";
}

var atomicWriteCounter = 0;

function cleanupAtomicResidues(file, minimumAgeMs) {
    try {
        var prefix = "." + file.name + ".tmp-";
        var backupPrefix = "." + file.name + ".bak-";
        var now = new Date().getTime();
        var minimumAge = typeof minimumAgeMs === "number" ? minimumAgeMs : 60 * 60 * 1000;
        var entries = file.parent.getFiles();
        for (var i = 0; i < entries.length; i++) {
            var entry = entries[i];
            if (!(entry instanceof File) ||
                (entry.name.indexOf(prefix) !== 0 && entry.name.indexOf(backupPrefix) !== 0)) {
                continue;
            }
            var modified = entry.modified ? entry.modified.getTime() : now;
            if (now - modified >= minimumAge) {
                try { entry.remove(); } catch (_cleanupErr) {}
            }
        }
    } catch (_scanErr) {}
}

function writeAtomicTextFile(file, text) {
    if (file.parent && !file.parent.exists && !file.parent.create()) {
        throw new Error("Failed to create directory: " + file.parent.fsName);
    }
    cleanupAtomicResidues(file);
    // File.rename mutates the ExtendScript File object's name/path. Capture the
    // destination name before preserving the current target so the subsequent
    // publish and rollback still address the original file.
    var targetName = file.name;
    atomicWriteCounter += 1;
    var suffix = new Date().getTime() + "-" + atomicWriteCounter + "-" +
        Math.floor(Math.random() * 0x7fffffff).toString(16);
    var tempFile = new File(file.parent.fsName + "/." + file.name + ".tmp-" + suffix);
    tempFile.encoding = "UTF-8";
    if (!tempFile.open("w")) {
        throw new Error("Failed to open temporary file: " + tempFile.fsName);
    }
    if (!tempFile.write(text)) {
        try { tempFile.close(); } catch (_writeCloseErr) {}
        try { tempFile.remove(); } catch (_writeRemoveErr) {}
        throw new Error("Failed to write temporary file: " + tempFile.fsName);
    }
    // ExtendScript exposes no fsync; a successful close is the host flush boundary.
    if (!tempFile.close()) {
        try { tempFile.remove(); } catch (_closeRemoveErr) {}
        throw new Error("Failed to flush temporary file: " + tempFile.fsName);
    }

    // Prefer a direct same-directory rename. Hosts that allow overwrite make this atomic.
    if (tempFile.rename(targetName)) {
        return;
    }

    // Some legacy hosts reject overwrite. Keep the old valid file as a backup and
    // restore it if publishing fails. Readers retry a transient missing target.
    var backupPath = file.parent.fsName + "/." + file.name + ".bak-" + suffix;
    var backupFile = new File(backupPath);
    var hadTarget = file.exists;
    if (hadTarget && !file.rename(backupFile.name)) {
        try { tempFile.remove(); } catch (_targetRemoveErr) {}
        throw new Error("Failed to preserve previous file: " + file.fsName);
    }
    if (!tempFile.rename(targetName)) {
        var rollbackFile = new File(backupPath);
        if (hadTarget && rollbackFile.exists) {
            try { rollbackFile.rename(targetName); } catch (_rollbackErr) {}
        }
        try { tempFile.remove(); } catch (_publishRemoveErr) {}
        throw new Error("Failed to publish temporary file: " + file.fsName);
    }
    var publishedBackup = new File(backupPath);
    if (publishedBackup.exists) {
        try { publishedBackup.remove(); } catch (_backupRemoveErr) {}
    }
}

function writeHeartbeatTextFile(file, text) {
    if (file.parent && !file.parent.exists && !file.parent.create()) {
        throw new Error("Failed to create directory: " + file.parent.fsName);
    }
    // ExtendScript cannot replace an existing file with File.rename on
    // Windows. The atomic fallback therefore creates a short missing-file
    // window that is unacceptable for liveness discovery. Keep heartbeat.json
    // present and rely on the Rust reader's retry for a transient partial JSON
    // read. Heartbeat writes are serialized by the host's script engine.
    cleanupAtomicResidues(file, 0);
    file.encoding = "UTF-8";
    if (!file.open("w")) {
        throw new Error("Failed to open heartbeat file: " + file.fsName);
    }
    if (!file.write(text)) {
        try { file.close(); } catch (_heartbeatWriteCloseErr) {}
        throw new Error("Failed to write heartbeat file: " + file.fsName);
    }
    if (!file.close()) {
        throw new Error("Failed to close heartbeat file: " + file.fsName);
    }
}

function getDebugConfigFilePath() {
    return getBridgeFolderPath() + "/ae_mcp_debug_config.json";
}

function getDefaultDebugLogFilePath() {
    try {
        if ($.os && $.os.toLowerCase().indexOf("windows") >= 0) {
            var installedRoot = new Folder("C:/Program Files/AfterEffectsMcp");
            if (installedRoot.exists) {
                return installedRoot.fsName + "/ae_mcp_debug.log";
            }
        }
    } catch (_osErr) {}
    return getBridgeFolderPath() + "/ae_mcp_debug.log";
}

function getDebugLogFilePath() {
    if (debugLogPathOverride && debugLogPathOverride !== "") {
        return debugLogPathOverride;
    }
    return getDefaultDebugLogFilePath();
}

function readDebugLogConfig() {
    var config = {
        enabled: false,
        logPath: getDefaultDebugLogFilePath()
    };
    try {
        var configFile = new File(getDebugConfigFilePath());
        configFile.encoding = "UTF-8";
        if (!configFile.exists) {
            return config;
        }
        if (!configFile.open("r")) {
            return config;
        }
        var raw = configFile.read();
        configFile.close();
        if (!raw) {
            return config;
        }
        var parsed = JSON.parse(raw);
        if (parsed) {
            config.enabled = parsed.enabled === true;
            if (parsed.logPath && typeof parsed.logPath === "string") {
                config.logPath = parsed.logPath;
            }
        }
    } catch (_e) {}
    return config;
}

function writeDebugLogConfig(enabled, logPath) {
    try {
        ensureBridgeFolder();
        var configFile = new File(getDebugConfigFilePath());
        writeAtomicTextFile(configFile, JSON.stringify({
            enabled: enabled === true,
            logPath: logPath || getDefaultDebugLogFilePath()
        }, null, 2));
    } catch (_e) {}
}

function applyDebugLogConfig(config) {
    config = config || readDebugLogConfig();
    debugLogEnabled = config.enabled === true;
    debugLogPathOverride = config.logPath || getDefaultDebugLogFilePath();
    try {
        if (isControlValid(debugLogCheckbox)) {
            debugLogCheckbox.value = debugLogEnabled;
        }
    } catch (_e) {}
}

function setDebugLogEnabled(enabled) {
    debugLogEnabled = enabled === true;
    if (!debugLogPathOverride) {
        debugLogPathOverride = getDefaultDebugLogFilePath();
    }
    writeDebugLogConfig(debugLogEnabled, debugLogPathOverride);
}

function resetDebugLogFileForSession() {
    if (!debugLogEnabled) {
        return;
    }
    try {
        var logFile = new File(getDebugLogFilePath());
        logFile.encoding = "UTF-8";
        try {
            if (logFile.parent && !logFile.parent.exists) {
                logFile.parent.create();
            }
        } catch (_parentErr) {}
        if (logFile.exists) {
            logFile.remove();
        }
        if (logFile.open("w")) {
            logFile.close();
        }
    } catch (_e) {}
}

function appendDebugLog(message) {
    if (!debugLogEnabled) {
        return;
    }
    try {
        var logPath = getDebugLogFilePath();
        var logFile = new File(logPath);
        logFile.encoding = "UTF-8";
        try {
            if (logFile.parent && !logFile.parent.exists) {
                logFile.parent.create();
            }
        } catch (_parentErr) {}
        if (logFile.open("a")) {
            logFile.writeln(fxDateToIsoString(new Date()) + " " + message);
            logFile.close();
        }
    } catch (_e) {}
}

function getProjectPathForHeartbeat() {
    try {
        if (app.project && app.project.file) {
            return app.project.file.fsName;
        }
    } catch (_e) {}
    return "";
}

function getBridgeInstanceStatus() {
    if (!bridgeRuntimeRunning) {
        return "stopped";
    }
    if (currentRequestId) {
        return "running";
    }
    try {
        if (isControlValid(autoRunCheckbox) && !autoRunCheckbox.value) {
            return "paused";
        }
    } catch (_e) {}
    return "idle";
}

function getHostInstanceMetadata() {
    var bridgeRoot = ensureBridgeFolder().fsName;
    var updatedAt = fxDateToIsoString(new Date());
    return {
        protocolVersion: 1,
        instanceId: getBridgeInstanceId(),
        appName: "After Effects",
        appVersion: (app && app.version) ? String(app.version) : "",
        displayName: "Adobe After Effects " + ((app && app.version) ? String(app.version) : ""),
        bridgeVersion: AE_MCP_BRIDGE_VERSION,
        bridgeRuntime: aeMcpHeadless ? "extendscript-startup" : "extendscript-scriptui",
        lifecycleMode: aeMcpLifecycleMode,
        runtimeId: bridgeRuntimeId,
        runtimeStartedAt: bridgeStartedAt,
        hostId: "aftereffects",
        capabilities: ["run-jsx", "projects.read", "compositions.read", "layers.read", "render-queue"],
        projectPath: getProjectPathForHeartbeat(),
        status: getBridgeInstanceStatus(),
        currentRequestId: currentRequestId || null,
        currentCommandName: currentCommandName || null,
        bridgeRoot: bridgeRoot,
        commandFile: getCommandFilePath(),
        resultFile: getResultFilePath(),
        lastHeartbeatAt: updatedAt,
        updatedAt: updatedAt
    };
}

function writeInstanceHeartbeat() {
    try {
        var heartbeatFile = new File(getHeartbeatFilePath());
        writeHeartbeatTextFile(heartbeatFile, JSON.stringify(getHostInstanceMetadata(), null, 2));
    } catch (heartbeatError) {
        logToPanel("Failed to write instance heartbeat: " + heartbeatError.toString());
    }
}

function heartbeatTick() {
    writeInstanceHeartbeat();
}

function hasFileNetworkAccessPermission() {
    var probeFile = null;
    try {
        var bridgeFolder = ensureBridgeFolder();
        probeFile = new File(bridgeFolder.fsName + "/.ae_mcp_permission_probe");
        probeFile.encoding = "UTF-8";
        if (!probeFile.open("w")) {
            return false;
        }
        probeFile.write("ok");
        probeFile.close();
        if (probeFile.exists) {
            probeFile.remove();
        }
        return true;
    } catch (_e) {
        try {
            if (probeFile && probeFile.opened) {
                probeFile.close();
            }
        } catch (_closeErr) {}
        return false;
    }
}

function showPermissionDialog() {
    var shouldResume = commandCheckerTaskId !== 0;
    if (shouldResume) {
        stopCommandChecker();
    }
    alert(
        "MCP Bridge Auto cannot access files/network.\n\n" +
        "Enable \"Allow Scripts to Write Files and Access Network\" in:\n" +
        "Edit > Preferences > Scripting & Expressions\n\n" +
        "After enabling, restart After Effects or reopen this panel."
    );
    if (shouldResume) {
        startCommandChecker();
    }
}

function isModalDialogError(error) {
    var message = "";
    try {
        message = (error && error.toString) ? error.toString() : ("" + error);
    } catch (_e) {
        return false;
    }
    var normalized = message.toLowerCase();
    return normalized.indexOf("cannot run a script while a modal dialog") >= 0 ||
           normalized.indexOf("modal dialog") >= 0;
}

function applyPermissionState(showDialog) {
    if (!hasFileNetworkPermission) {
        if (isControlValid(autoRunCheckbox)) {
            if (autoRunCheckbox.enabled) {
                autoRunValueBeforePermissionLock = autoRunCheckbox.value;
            }
            autoRunCheckbox.value = false;
            autoRunCheckbox.enabled = false;
        }
        if (isControlValid(checkButton)) {
            checkButton.enabled = false;
        }
        setStatus("Permission required - Enable Scripts to Write Files and Access Network");

        if (showDialog && !aeMcpHeadless && !hasShownPermissionDialog) {
            hasShownPermissionDialog = true;
            showPermissionDialog();
        }
        return;
    }

    var wasLocked = false;
    if (isControlValid(autoRunCheckbox)) {
        wasLocked = !autoRunCheckbox.enabled;
        autoRunCheckbox.enabled = true;
        if (wasLocked) {
            autoRunCheckbox.value = autoRunValueBeforePermissionLock;
        }
    }
    if (isControlValid(checkButton)) {
        wasLocked = wasLocked || !checkButton.enabled;
        checkButton.enabled = true;
    }
    if (wasLocked) {
        hasShownPermissionDialog = false;
        logToPanel("File/network access detected. Controls re-enabled.");
    }
    setStatus("Ready - Auto-run is " + (isAutoRunEnabled() ? "ON" : "OFF"));
}

function refreshPermissionState(showDialog) {
    var granted = hasFileNetworkAccessPermission();
    if (!permissionStateKnown || granted !== hasFileNetworkPermission) {
        permissionStateKnown = true;
        hasFileNetworkPermission = granted;
        if (granted) {
            logToPanel("File/network access is enabled.");
        } else {
            logToPanel("File/network access is disabled.");
        }
    }
    applyPermissionState(showDialog);
    return hasFileNetworkPermission;
}

// Functions for each script type
function getProjectInfo() {
    var project = app.project;
    var result = {
        projectName: project.file ? project.file.name : "Untitled Project",
        path: project.file ? project.file.fsName : "",
        numItems: project.numItems,
        bitsPerChannel: project.bitsPerChannel,
        timeMode: project.timeDisplayType === TimeDisplayType.FRAMES ? "Frames" : "Timecode",
        items: []
    };

    // Count item types
    var countByType = {
        compositions: 0,
        footage: 0,
        folders: 0,
        solids: 0
    };

    // Get item information (limited for performance)
    for (var i = 1; i <= Math.min(project.numItems, 50); i++) {
        var item = project.item(i);
        var itemType = "";
        
        if (item instanceof CompItem) {
            itemType = "Composition";
            countByType.compositions++;
        } else if (item instanceof FolderItem) {
            itemType = "Folder";
            countByType.folders++;
        } else if (item instanceof FootageItem) {
            if (item.mainSource instanceof SolidSource) {
                itemType = "Solid";
                countByType.solids++;
            } else {
                itemType = "Footage";
                countByType.footage++;
            }
        }
        
        result.items.push({
            id: item.id,
            name: item.name,
            type: itemType
        });
    }
    
    result.itemCounts = countByType;

    // Include active composition metadata if available
    if (app.project.activeItem instanceof CompItem) {
        var ac = app.project.activeItem;
        result.activeComp = {
            id: ac.id,
            name: ac.name,
            width: ac.width,
            height: ac.height,
            duration: ac.duration,
            frameRate: ac.frameRate,
            numLayers: ac.numLayers
        };
    }

    return JSON.stringify(result, null, 2);
}

function listCompositions() {
    var project = app.project;
    var result = {
        compositions: []
    };
    
    // Loop through items in the project
    for (var i = 1; i <= project.numItems; i++) {
        var item = project.item(i);
        
        // Check if the item is a composition
        if (item instanceof CompItem) {
            result.compositions.push({
                id: item.id,
                name: item.name,
                duration: item.duration,
                frameRate: item.frameRate,
                width: item.width,
                height: item.height,
                numLayers: item.numLayers
            });
        }
    }
    
    return JSON.stringify(result, null, 2);
}

function getLayerInfo() {
    var project = app.project;
    var result = {
        layers: []
    };
    
    // Get the active composition
    var activeComp = null;
    if (app.project.activeItem instanceof CompItem) {
        activeComp = app.project.activeItem;
    } else {
        return JSON.stringify({ error: "No active composition" }, null, 2);
    }
    
    // Loop through layers in the active composition
    for (var i = 1; i <= activeComp.numLayers; i++) {
        var layer = activeComp.layer(i);
        var layerInfo = {
            id: fxGetLayerId(layer),
            index: layer.index,
            name: layer.name,
            enabled: layer.enabled,
            locked: layer.locked,
            inPoint: layer.inPoint,
            outPoint: layer.outPoint
        };
        
        result.layers.push(layerInfo);
    }
    
    return JSON.stringify(result, null, 2);
}

// Execute command
function fxMakeJsonSafe(value) {
    if (typeof value === "undefined") {
        return null;
    }
    try {
        return JSON.parse(JSON.stringify(value));
    } catch (_jsonErr) {
        return fxSerializeValue(value);
    }
}

function executeJsx(args) {
    var description = "";
    var undoStarted = false;
    try {
        args = args || {};
        var code = args.code;
        var mode = args.mode;
        description = args.description || "";
        if (mode !== "unsafe" && mode !== "trusted") {
            throw new Error("executeJsx requires mode='unsafe' or mode='trusted'");
        }
        if (!description) {
            throw new Error("executeJsx requires a non-empty description");
        }
        if (!code || typeof code !== "string") {
            throw new Error("executeJsx requires string code");
        }

        var userArgs = args.args || {};
        var sourcePath = args.sourcePath || "";
        var mcp = {
            log: function (message) {
                logToPanel("[executeJsx] " + message);
            },
            shouldCancel: function () {
                return false;
            }
        };

        logToPanel("executeJsx started: " + description + (sourcePath ? " (" + sourcePath + ")" : ""));
        app.beginUndoGroup(description);
        undoStarted = true;
        var result = (function (args, mcp) {
            return eval(code);
        })(userArgs, mcp);

        if (undoStarted) {
            app.endUndoGroup();
            undoStarted = false;
        }
        logToPanel("executeJsx completed: " + description);
        return JSON.stringify({
            status: "success",
            description: description,
            mode: mode,
            sourcePath: sourcePath,
            sourceSha256: args.sourceSha256 || "",
            sourceSizeBytes: args.sourceSizeBytes,
            result: fxMakeJsonSafe(result)
        }, null, 2);
    } catch (error) {
        if (undoStarted) {
            try {
                app.endUndoGroup();
            } catch (_undoErr) {}
        }
        logToPanel("executeJsx error: " + error.toString());
        return JSON.stringify({
            status: "error",
            description: description,
            message: error.toString(),
            line: error.line,
            fileName: error.fileName
        }, null, 2);
    }
}

function executeJsxFile(args) {
    try {
        args = args || {};
        var path = args.path || "";
        if (!path) {
            throw new Error("executeJsxFile requires path");
        }
        var file = new File(path);
        file.encoding = "UTF-8";
        if (!file.exists) {
            throw new Error("JSX file not found: " + path);
        }
        if (!file.open("r")) {
            throw new Error("Unable to open JSX file: " + path);
        }
        var code = file.read();
        file.close();
        args.code = code;
        args.sourcePath = file.fsName;
        return executeJsx(args);
    } catch (error) {
        logToPanel("executeJsxFile error: " + error.toString());
        return JSON.stringify({
            status: "error",
            message: error.toString(),
            line: error.line,
            fileName: error.fileName
        }, null, 2);
    }
}

function executeCommand(command, args, requestId) {
    var result = "";
    var shouldQuitAfterWrite = false;
    requestId = requestId || "";
    currentRequestId = requestId;
    currentCommandName = command;
    writeInstanceHeartbeat();
    
    logToPanel("Executing command: " + command);
    setStatus("Running: " + command);
    safePanelUpdate();
    
    try {
        logToPanel("Attempting to execute: " + command); // Log before switch
        // Use a switch statement for clarity
        switch (command) {
            case "executeJsx":
                logToPanel("Calling executeJsx function...");
                result = executeJsx(args);
                logToPanel("Returned from executeJsx.");
                break;
            case "executeJsxFile":
                logToPanel("Calling executeJsxFile function...");
                result = executeJsxFile(args);
                logToPanel("Returned from executeJsxFile.");
                break;
            case "getProjectInfo":
                result = getProjectInfo();
                break;
            case "listCompositions":
                result = listCompositions();
                break;
            case "getLayerInfo":
                result = getLayerInfo();
                break;
            case "createComposition":
                logToPanel("Calling createComposition function...");
                result = createComposition(args);
                logToPanel("Returned from createComposition.");
                break;
            case "createTextLayer":
                logToPanel("Calling createTextLayer function...");
                result = createTextLayer(args);
                logToPanel("Returned from createTextLayer.");
                break;
            case "createShapeLayer":
                logToPanel("Calling createShapeLayer function...");
                result = createShapeLayer(args);
                logToPanel("Returned from createShapeLayer. Result type: " + typeof result);
                break;
            case "createSolidLayer":
                logToPanel("Calling createSolidLayer function...");
                result = createSolidLayer(args);
                logToPanel("Returned from createSolidLayer.");
                break;
            case "setLayerProperties":
                logToPanel("Calling setLayerProperties function...");
                result = setLayerProperties(args);
                logToPanel("Returned from setLayerProperties.");
                break;
            case "setLayerKeyframe":
                logToPanel("Calling setLayerKeyframe function...");
                result = setLayerKeyframe(args.compIndex, args.layerIndex, args.propertyName, args.timeInSeconds, args.value);
                logToPanel("Returned from setLayerKeyframe.");
                break;
            case "setLayerExpression":
                logToPanel("Calling setLayerExpression function...");
                result = setLayerExpression(args.compIndex, args.layerIndex, args.propertyName, args.expressionString);
                logToPanel("Returned from setLayerExpression.");
                break;
            case "applyEffect":
                logToPanel("Calling applyEffect function...");
                result = applyEffect(args);
                logToPanel("Returned from applyEffect.");
                break;
            case "applyEffectTemplate":
                logToPanel("Calling applyEffectTemplate function...");
                result = applyEffectTemplate(args);
                logToPanel("Returned from applyEffectTemplate.");
                break;
            case "listSupportedEffects":
                logToPanel("Calling listSupportedEffects function...");
                result = listSupportedEffects(args);
                logToPanel("Returned from listSupportedEffects.");
                break;
            case "describeEffect":
                logToPanel("Calling describeEffect function...");
                result = describeEffect(args);
                logToPanel("Returned from describeEffect.");
                break;
            case "saveFramePng":
                logToPanel("Calling saveFramePng function...");
                result = saveFramePng(args);
                logToPanel("Returned from saveFramePng.");
                break;
            case "renderQueueAdd":
                logToPanel("Calling renderQueueAdd function...");
                result = renderQueueAdd(args);
                logToPanel("Returned from renderQueueAdd.");
                break;
            case "renderQueueStatus":
                logToPanel("Calling renderQueueStatus function...");
                result = renderQueueStatus(args);
                logToPanel("Returned from renderQueueStatus.");
                break;
            case "renderQueueStart":
                logToPanel("Calling renderQueueStart function...");
                result = renderQueueStart(args);
                logToPanel("Returned from renderQueueStart.");
                break;
            case "renderQueueIsRendering":
                logToPanel("Calling renderQueueIsRendering function...");
                result = renderQueueIsRendering(args);
                logToPanel("Returned from renderQueueIsRendering.");
                break;
            case "setCurrentTime":
                logToPanel("Calling setCurrentTime function...");
                result = setCurrentTime(args);
                logToPanel("Returned from setCurrentTime.");
                break;
            case "getCurrentTime":
                logToPanel("Calling getCurrentTime function...");
                result = getCurrentTime(args);
                logToPanel("Returned from getCurrentTime.");
                break;
            case "setWorkArea":
                logToPanel("Calling setWorkArea function...");
                result = setWorkArea(args);
                logToPanel("Returned from setWorkArea.");
                break;
            case "getWorkArea":
                logToPanel("Calling getWorkArea function...");
                result = getWorkArea(args);
                logToPanel("Returned from getWorkArea.");
                break;
            case "getCompositionMarkers":
                logToPanel("Calling getCompositionMarkers function...");
                result = getCompositionMarkers(args);
                logToPanel("Returned from getCompositionMarkers.");
                break;
            case "cleanupPreviewFolder":
                logToPanel("Calling cleanupPreviewFolder function...");
                result = cleanupPreviewFolder(args);
                logToPanel("Returned from cleanupPreviewFolder.");
                break;
            case "setSuppressDialogs":
                logToPanel("Calling setSuppressDialogs function...");
                result = setSuppressDialogs(args);
                logToPanel("Returned from setSuppressDialogs.");
                break;
            case "getSuppressDialogs":
                logToPanel("Calling getSuppressDialogs function...");
                result = getSuppressDialogs();
                logToPanel("Returned from getSuppressDialogs.");
                break;
            case "projectOpen":
                logToPanel("Calling projectOpen function...");
                result = projectOpen(args);
                logToPanel("Returned from projectOpen.");
                break;
            case "projectClose":
                logToPanel("Calling projectClose function...");
                result = projectClose(args);
                logToPanel("Returned from projectClose.");
                break;
            case "projectSave":
                logToPanel("Calling projectSave function...");
                result = projectSave(args);
                logToPanel("Returned from projectSave.");
                break;
            case "projectSaveAs":
                logToPanel("Calling projectSaveAs function...");
                result = projectSaveAs(args);
                logToPanel("Returned from projectSaveAs.");
                break;
            case "applicationQuit":
                logToPanel("Calling applicationQuit function...");
                result = applicationQuit(args);
                logToPanel("Returned from applicationQuit.");
                break;
            case "bridgeTestEffects":
                logToPanel("Calling bridgeTestEffects function...");
                result = bridgeTestEffects(args);
                logToPanel("Returned from bridgeTestEffects.");
                break;
            default:
                result = JSON.stringify({ error: "Unknown command: " + command });
        }
        logToPanel("Execution finished for: " + command); // Log after switch
        
        // Save the result (ensure result is always a string)
        logToPanel("Preparing to write result file...");
        var resultString = (typeof result === 'string') ? result : JSON.stringify(result);
        
        // Try to parse the result as JSON to add a timestamp
        try {
            var resultObj = JSON.parse(resultString);
            if (resultObj && resultObj._quitRequested === true) {
                shouldQuitAfterWrite = true;
                try {
                    delete resultObj._quitRequested;
                } catch (_deleteQuitFlagErr) {}
            }
            // Add a timestamp to help identify if we're getting fresh results
            resultObj._responseTimestamp = fxDateToIsoString(new Date());
            resultObj._commandExecuted = command;
            resultObj._requestId = requestId;
            resultObj._hostInstance = getHostInstanceMetadata();
            resultString = JSON.stringify(resultObj, null, 2);
            logToPanel("Added timestamp to result JSON for tracking freshness.");
        } catch (parseError) {
            logToPanel("Could not parse result as JSON to add timestamp: " + parseError.toString());
            resultString = JSON.stringify({
                status: "success",
                result: resultString,
                _responseTimestamp: fxDateToIsoString(new Date()),
                _commandExecuted: command,
                _requestId: requestId,
                _hostInstance: getHostInstanceMetadata()
            }, null, 2);
        }
        
        var resultFile = new File(getResultFilePath());
        logToPanel("Writing result file atomically...");
        writeAtomicTextFile(resultFile, resultString);
        logToPanel("Result file write process complete.");
        
        logToPanel("Command completed successfully: " + command); // Changed log message
        setStatus("Command completed: " + command);
        
        // Update command file status
        logToPanel("Updating command status to completed...");
        updateCommandStatus("completed");
        logToPanel("Command status updated.");
        currentRequestId = "";
        currentCommandName = "";
        writeInstanceHeartbeat();

        if (shouldQuitAfterWrite) {
            try {
                logToPanel("Scheduling graceful application quit.");
                app.scheduleTask("app.quit()", 300, false);
            } catch (quitScheduleError) {
                logToPanel("Failed to schedule app.quit(): " + quitScheduleError.toString());
            }
        }
        
    } catch (error) {
        if (isModalDialogError(error)) {
            logToPanel("Modal dialog detected. Command will be retried automatically.");
            setStatus("Paused: modal dialog is open. Waiting to retry...");
            try {
                updateCommandStatus("pending");
            } catch (_retryStateErr) {}
            return;
        }

        var errorMsg = "ERROR in executeCommand for '" + command + "': " + error.toString() + (error.line ? " (line: " + error.line + ")" : "");
        logToPanel(errorMsg); // Log detailed error
        setStatus("Error: " + error.toString());
        
        // Write detailed error to result file
        try {
            logToPanel("Attempting to write ERROR to result file...");
            var errorResult = JSON.stringify({ 
                status: "error", 
                command: command,
                _commandExecuted: command,
                _requestId: requestId,
                _hostInstance: getHostInstanceMetadata(),
                _responseTimestamp: fxDateToIsoString(new Date()),
                message: error.toString(),
                line: error.line,
                fileName: error.fileName
            });
            var errorFile = new File(getResultFilePath());
            writeAtomicTextFile(errorFile, errorResult);
            logToPanel("Successfully wrote ERROR to result file.");
        } catch (writeError) {
             logToPanel("CRITICAL ERROR: Failed to write error to result file: " + writeError.toString());
        }
        
        // Update command file status even after error
        logToPanel("Updating command status to error...");
        updateCommandStatus("error");
        logToPanel("Command status updated to error.");
        currentRequestId = "";
        currentCommandName = "";
        writeInstanceHeartbeat();
    }
}

// Update command file status
function updateCommandStatus(status) {
    try {
        var commandFile = new File(getCommandFilePath());
        if (commandFile.exists) {
            commandFile.open("r");
            var content = commandFile.read();
            commandFile.close();
            
            if (content) {
                var commandData = JSON.parse(content);
                commandData.status = status;
                writeAtomicTextFile(commandFile, JSON.stringify(commandData, null, 2));
            }
        }
    } catch (e) {
        logToPanel("Error updating command status: " + e.toString());
    }
}

// Log message to panel
function logToPanel(message) {
    var timestamp = new Date().toLocaleTimeString();
    appendDebugLog(message);
    try {
        if (!isControlValid(logText)) {
            return;
        }
        logText.text = timestamp + ": " + message + "\n" + logText.text;
    } catch (_e) {}
}

// Check for new commands
function checkForCommands() {
    if (!isControlValid(autoRunCheckbox) || !isControlValid(checkButton)) {
        logCommandCheckerState(
            "controls-unavailable",
            "Command checker continuing without visible panel controls."
        );
    }
    if (isChecking) {
        logCommandCheckerState("already-checking", "Command checker skipped: previous check is still running.");
        return;
    }
    if (!refreshPermissionState(false)) {
        logCommandCheckerState("permission-missing", "Command checker paused: file/network permission is missing.");
        return;
    }
    writeInstanceHeartbeat();
    if (!isAutoRunEnabled()) {
        setStatus("Ready - Auto-run is OFF");
        logCommandCheckerState("auto-run-off", "Command checker paused: Auto-run commands is OFF.");
        return;
    }
    
    isChecking = true;
    
    try {
        var commandFile = new File(getCommandFilePath());
        if (commandFile.exists) {
            commandFile.open("r");
            var content = commandFile.read();
            commandFile.close();
            
            if (content) {
                var commandData = (typeof JSON !== "undefined" && JSON.parse)
                    ? JSON.parse(content)
                    : eval("(" + content + ")");
                
                // Only execute pending commands
                if (commandData.status === "pending") {
                    logCommandCheckerState(
                        "pending:" + commandData.command,
                        "Command checker picked pending command: " + commandData.command
                    );
                    // Update status to running
                    updateCommandStatus("running");
                    
                    // Execute the command
                    executeCommand(commandData.command, commandData.args || {}, commandData.requestId || "");
                } else {
                    logCommandCheckerState(
                        "status:" + commandData.status + ":" + commandData.command,
                        "Command checker saw command status=" + commandData.status + " command=" + commandData.command
                    );
                }
            } else {
                logCommandCheckerState("empty-command-file", "Command checker saw an empty command file.");
            }
        } else {
            logCommandCheckerState("no-command-file", "Command checker waiting: command file does not exist.");
        }
    } catch (e) {
        logToPanel("Error checking for commands: " + e.toString());
    } finally {
        isChecking = false;
    }
}

// Set up timer to check for commands
function startCommandChecker() {
    stopCommandChecker();
    commandCheckerTaskId = app.scheduleTask(
        "$.global.__adobeMcpBridgeCommandTick('" + bridgeRuntimeId + "')",
        checkInterval,
        true
    );
    if (bridgeRuntime) {
        bridgeRuntime.commandTaskId = commandCheckerTaskId;
    }
    logToPanel("Command checker scheduled. taskId=" + commandCheckerTaskId + " intervalMs=" + checkInterval);
}

function stopCommandChecker() {
    if (!commandCheckerTaskId) {
        return;
    }
    logToPanel("Stopping command checker. taskId=" + commandCheckerTaskId);
    try {
        app.cancelTask(commandCheckerTaskId);
    } catch (_e) {}
    commandCheckerTaskId = 0;
    if (bridgeRuntime) {
        bridgeRuntime.commandTaskId = 0;
    }
}

function startHeartbeatTask() {
    stopHeartbeatTask();
    writeInstanceHeartbeat();
    heartbeatTaskId = app.scheduleTask(
        "$.global.__adobeMcpBridgeHeartbeatTick('" + bridgeRuntimeId + "')",
        heartbeatInterval,
        true
    );
    if (bridgeRuntime) {
        bridgeRuntime.heartbeatTaskId = heartbeatTaskId;
    }
    logToPanel("Heartbeat task scheduled. taskId=" + heartbeatTaskId + " intervalMs=" + heartbeatInterval);
}

function stopHeartbeatTask() {
    if (!heartbeatTaskId) {
        return;
    }
    logToPanel("Stopping heartbeat task. taskId=" + heartbeatTaskId);
    try {
        app.cancelTask(heartbeatTaskId);
    } catch (_e) {}
    heartbeatTaskId = 0;
    if (bridgeRuntime) {
        bridgeRuntime.heartbeatTaskId = 0;
    }
}

function removeInstanceHeartbeat() {
    try {
        var heartbeatFile = new File(getHeartbeatFilePath());
        if (heartbeatFile.exists) {
            heartbeatFile.remove();
        }
    } catch (_heartbeatRemoveErr) {}
}

function getBridgeRuntimeState() {
    return {
        version: AE_MCP_BRIDGE_VERSION,
        running: bridgeRuntimeRunning,
        lifecycleMode: aeMcpLifecycleMode,
        runtimeId: bridgeRuntimeId,
        instanceId: getBridgeInstanceId(),
        commandTaskId: commandCheckerTaskId,
        heartbeatTaskId: heartbeatTaskId,
        startedAt: bridgeStartedAt,
        bootstrapSource: aeMcpBootstrapConfig.source || "manual"
    };
}

function stopBridgeRuntime(options) {
    options = options || {};
    stopCommandChecker();
    stopHeartbeatTask();
    bridgeRuntimeRunning = false;
    if (bridgeRuntime) {
        bridgeRuntime.running = false;
        bridgeRuntime.stoppedAt = fxDateToIsoString(new Date());
        bridgeRuntime.stopReason = options.reason || "requested";
    }
    if (options.removeHeartbeat !== false) {
        removeInstanceHeartbeat();
    }
    setStatus("Bridge stopped");
    return getBridgeRuntimeState();
}

function registerScheduledBridgeFunctions() {
    $.global.__adobeMcpBridgeCommandTick = function (scheduledRuntimeId) {
        if (!bridgeRuntimeRunning || scheduledRuntimeId !== bridgeRuntimeId) {
            return;
        }
        checkForCommands();
    };
    $.global.__adobeMcpBridgeHeartbeatTick = function (scheduledRuntimeId) {
        if (!bridgeRuntimeRunning || scheduledRuntimeId !== bridgeRuntimeId) {
            return;
        }
        heartbeatTick();
    };
    // Neutralize callbacks left by the pre-generation runtime. New scheduled
    // callbacks always use the generation-aware functions above.
    $.global.checkForCommands = function () { return null; };
    $.global.aeMcpHeartbeatTick = function () { return null; };
}

function startBridgeRuntime() {
    if (bridgeRuntimeRunning && commandCheckerTaskId && heartbeatTaskId) {
        writeInstanceHeartbeat();
        return getBridgeRuntimeState();
    }
    if (bridgeHasStarted) {
        bridgeRuntimeId = "runtime-" + (new Date().getTime()) + "-" + Math.floor(Math.random() * 1000000);
        bridgeStartedAt = fxDateToIsoString(new Date());
        if (bridgeRuntime) {
            bridgeRuntime.runtimeId = bridgeRuntimeId;
            bridgeRuntime.startedAt = bridgeStartedAt;
        }
    }
    bridgeHasStarted = true;
    bridgeRuntimeRunning = true;
    if (bridgeRuntime) {
        bridgeRuntime.running = true;
        bridgeRuntime.stoppedAt = null;
        bridgeRuntime.stopReason = null;
    }
    registerScheduledBridgeFunctions();
    refreshPermissionState(false);
    startHeartbeatTask();
    startCommandChecker();
    return getBridgeRuntimeState();
}

function restartBridgeRuntime() {
    stopBridgeRuntime({ removeHeartbeat: false, reason: "restart" });
    return startBridgeRuntime();
}

applyDebugLogConfig(readDebugLogConfig());
resetDebugLogFileForSession();

try {
    bridgeRuntime = {
        version: AE_MCP_BRIDGE_VERSION,
        running: false,
        lifecycleMode: aeMcpLifecycleMode,
        runtimeId: bridgeRuntimeId,
        startedAt: bridgeStartedAt,
        commandTaskId: 0,
        heartbeatTaskId: 0,
        start: startBridgeRuntime,
        stop: stopBridgeRuntime,
        restart: restartBridgeRuntime,
        getState: getBridgeRuntimeState,
        writeHeartbeat: writeInstanceHeartbeat
    };
    $.global.__adobeMcpBridgeRuntime = bridgeRuntime;
    $.global.aeMcpBridgeStart = startBridgeRuntime;
    $.global.aeMcpBridgeStop = stopBridgeRuntime;
    $.global.aeMcpBridgeRestart = restartBridgeRuntime;
    $.global.aeMcpBridgeGetState = getBridgeRuntimeState;
    registerScheduledBridgeFunctions();
    logToPanel("Registered bridge lifecycle and scheduled functions on $.global.");
} catch (globalRegisterError) {
    logToPanel("Failed to register bridge lifecycle on $.global: " + globalRegisterError.toString());
}

// Add manual check button
if (panel) {
    checkButton = panel.add("button", undefined, "Check for Commands Now");
    checkButton.onClick = function() {
        if (!refreshPermissionState(true)) {
            return;
        }
        logToPanel("Manually checking for commands");
        checkForCommands();
    };

    debugLogCheckbox.onClick = function() {
        setDebugLogEnabled(debugLogCheckbox.value === true);
        logToPanel(
            "Debug file logging " +
            (debugLogEnabled ? "enabled: " + getDebugLogFilePath() : "disabled")
        );
    };
}

// Log startup
logToPanel("MCP Bridge Auto started");
logToPanel("Lifecycle mode: " + aeMcpLifecycleMode);
try {
    logToPanel("AE instance: " + getBridgeInstanceId());
    logToPanel("Command file: " + getCommandFilePath());
    logToPanel("Heartbeat file: " + getHeartbeatFilePath());
    logToPanel("Debug file logging: " + (debugLogEnabled ? "ON" : "OFF"));
    logToPanel("Debug log file: " + getDebugLogFilePath());
} catch (pathError) {
    logToPanel("Command file path unavailable: " + pathError.toString());
}
refreshPermissionState(!aeMcpHeadless);

// Start the command checker
startBridgeRuntime();

if (panel) {
    panel.onClose = function () {
        stopBridgeRuntime({ removeHeartbeat: true, reason: "panel-closed" });
        return true;
    };

    panel.onResizing = panel.onResize = function () {
        this.layout.resize();
    };
    panel.layout.layout(true);

    if (!isDockablePanel) {
        panel.center();
        panel.show();
    }
}

})(this);
