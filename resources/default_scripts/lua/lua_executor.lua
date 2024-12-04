local json = require("json")
local log = require("logging_utils")

-- Safely execute Lua code with restrictions
function safe_execute(code)
    log.debug("Executing Lua code block with safety restrictions")
    
    -- Create a new environment with limited functions
    local env = {
        print = print,
        string = string,
        table = table,
        math = math,
        tonumber = tonumber,
        tostring = tostring,
        type = type,
        select = select,
        pairs = pairs,
        ipairs = ipairs,
    }
    
    -- Create the function with restricted environment
    local func, err = load(code, "code block", "t", env)
    if not func then
        log.error("Failed to load code block: {}", err)
        return "Error: " .. err
    end
    
    -- Capture output
    local outputs = {}
    env.print = function(...)
        local args = {...}
        local str_args = {}
        for i, v in ipairs(args) do
            str_args[i] = tostring(v)
        end
        table.insert(outputs, table.concat(str_args, "\t"))
    end
    
    -- Execute and return results
    local success, result = pcall(func)
    if not success then
        log.error("Failed to execute code block: {}", result)
        return "Error: " .. result
    end
    
    local output = table.concat(outputs, "\n")
    log.trace("Code block execution output: {}", output)
    return output
end

-- Extract and process Lua code blocks
function process_lua_blocks(content)
    log.debug("Processing Lua code blocks in content")
    local modified_content = content
    local has_changes = false
    local blocks_processed = 0
    
    -- Process each Lua code block
    local pos = 1
    while true do
        -- First try to find a code block with existing output
        local start, finish = modified_content:find("```lua\n.-\n```\n\n> Output:\n>[^\n]*\n", pos)
        
        if not start then
            -- Try to find a lone code block without output
            local code_start, code_finish, code = modified_content:find("```lua\n(.-)\n```", pos)
            if not code_start then break end
            
            log.debug("Found new Lua code block at position {}", code_start)
            
            -- Execute the code
            local output = safe_execute(code)
            
            -- Format the output block
            local output_block = string.format("```lua\n%s\n```\n\n> Output:\n> %s\n",
                code,
                output)
            
            -- Replace the code block with code + output
            modified_content = modified_content:sub(1, code_start-1) .. output_block .. modified_content:sub(code_finish+1)
            pos = code_start + #output_block
            has_changes = true
            blocks_processed = blocks_processed + 1
            log.trace("Processed new code block: {}", code)
        else
            -- Extract the code and existing output
            local code_block = modified_content:sub(start, finish)
            local _, _, code = code_block:find("```lua\n(.-)\n```")
            
            log.debug("Found existing Lua code block at position {}", start)
            
            -- Execute the code
            local output = safe_execute(code)
            
            -- Format the new block
            local output_block = string.format("```lua\n%s\n```\n\n> Output:\n> %s\n",
                code,
                output)
            
            -- Replace the entire block
            modified_content = modified_content:sub(1, start-1) .. output_block .. modified_content:sub(finish+1)
            pos = start + #output_block
            has_changes = true
            blocks_processed = blocks_processed + 1
            log.trace("Re-processed existing code block: {}", code)
        end
    end
    
    -- Clean up any extra newlines
    modified_content = modified_content:gsub("\n\n\n+", "\n\n")
    
    if blocks_processed > 0 then
        log.info("✨ Processed {} Lua code blocks", blocks_processed)
    else
        log.debug("No Lua code blocks found in content")
    end
    
    return modified_content, has_changes
end

function on_event(event_json)
    log.debug("Processing event for Lua execution")
    
    local success, event = pcall(json.decode, event_json)
    if not success then
        log.error("Failed to decode event JSON: {}", event)
        return nil
    end
    
    local event_data = event.Created or event.Updated or event.Synced
    
    if event_data and event_data.content then
        local title = event_data.title or "unknown"
        log.debug("Processing content from note: {}", title)
        
        local new_content, has_changes = process_lua_blocks(event_data.content)
        
        if has_changes then
            log.info("🔧 Updated Lua code blocks in '{}'", title)
            local result = {
                metadata = {
                    lua_blocks_executed = "true",
                    last_executed = os.date("%Y-%m-%d %H:%M:%S")
                },
                content = new_content
            }
            log.debug("Generated result with updated content and metadata")
            return json.encode(result)
        else
            log.debug("No changes made to content")
        end
    else
        log.debug("No suitable content found for processing")
    end
    
    return nil
end